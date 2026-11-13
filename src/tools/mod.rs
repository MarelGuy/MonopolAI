//! Tools available to the `MonopolAI` `ReAct` agent.
//!
//! The registry is the only place that knows about the game edition. Adding a
//! tool means:
//! 1. adding a definition to [`MonopolyTools::definitions`],
//! 2. adding a branch to [`MonopolyTools::execute_inner`].
//!
//! Everything else (provider, `ReAct` loop, SSE) stays untouched.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::llm::{BoxFuture, ToolCall, ToolDefinition, ToolExecutor};
use crate::merror::{MError, MResult};
use crate::schemas::game::Edition;

pub struct MonopolyTools {
    edition: Edition,
}

impl MonopolyTools {
    #[must_use]
    pub const fn new(edition: Edition) -> Self {
        Self { edition }
    }

    /// Tool schemas advertised to the model.
    #[must_use]
    pub fn definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::new(
                "get_board_space",
                "Look up one Monopoly board space by its 0-based position or by its name. \
                 Returns the space type, colour, purchase cost and rent table.",
                json!({
                    "type": "object",
                    "properties": {
                        "position": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "0-based position on the board (0 = Start)."
                        },
                        "name": {
                            "type": "string",
                            "description": "Case-insensitive space name, e.g. 'Old Kent Road'."
                        }
                    },
                    "anyOf": [
                        { "required": ["position"] },
                        { "required": ["name"] }
                    ],
                    "additionalProperties": false
                }),
            ),
            ToolDefinition::new(
                "list_properties",
                "List the purchasable properties and stations of the board with their position, \
                 colour and cost. Optionally filter by colour.",
                json!({
                    "type": "object",
                    "properties": {
                        "color": {
                            "type": "string",
                            "description": "Optional colour filter, e.g. 'Brown', 'Dark Blue', 'Light Red'."
                        }
                    },
                    "additionalProperties": false
                }),
            ),
        ]
    }

    fn execute_inner(&self, call: &ToolCall) -> MResult<String> {
        match call.name.as_str() {
            "get_board_space" => self.get_board_space(&call.arguments),
            "list_properties" => self.list_properties(&call.arguments),
            other => Err(MError::bad_request(format!("Unknown tool `{other}`"))),
        }
    }

    fn get_board_space(&self, arguments: &Value) -> MResult<String> {
        let arguments: BoardSpaceArgs =
            serde_json::from_value(arguments.clone()).unwrap_or_default();

        let found = if let Some(position) = arguments.position {
            self.edition
                .spaces
                .get(position)
                .map(|space| (position, space))
        } else if let Some(name) = arguments.name.as_deref() {
            let name = name.trim().to_lowercase();
            self.edition.spaces.iter().enumerate().find(|(_, space)| {
                space
                    .name()
                    .is_some_and(|space_name| space_name.to_lowercase() == name)
            })
        } else {
            return Err(MError::bad_request("Provide either `position` or `name`"));
        };

        let Some((position, space)) = found else {
            return Err(MError::not_found("Board space not found"));
        };

        let space =
            serde_json::to_value(space).map_err(|error| MError::internal(error.to_string()))?;

        serde_json::to_string_pretty(&json!({
            "position": position,
            "space": space,
        }))
        .map_err(|error| MError::internal(error.to_string()))
    }

    fn list_properties(&self, arguments: &Value) -> MResult<String> {
        let arguments: ListPropertiesArgs =
            serde_json::from_value(arguments.clone()).unwrap_or_default();
        let filter = arguments
            .color
            .as_deref()
            .map(normalize_label)
            .filter(|color| !color.is_empty());

        let properties: Vec<Value> = self
            .edition
            .spaces
            .iter()
            .enumerate()
            .filter_map(|(position, space)| {
                // Purchasable spaces include coloured properties, stations and
                // utilities; the colour is `None` for the latter two.
                space.cost()?;
                let color_name = space.color().and_then(|color| {
                    serde_json::to_value(color)
                        .ok()
                        .and_then(|value| value.as_str().map(ToOwned::to_owned))
                });
                if filter.as_deref().is_some_and(|filter| {
                    color_name
                        .as_deref()
                        .is_none_or(|color| normalize_label(color) != filter)
                }) {
                    return None;
                }

                Some(json!({
                    "position": position,
                    "name": space.name(),
                    "type": space.space_type(),
                    "color": color_name,
                    "cost": space.cost(),
                }))
            })
            .collect();

        serde_json::to_string_pretty(&json!({ "properties": properties }))
            .map_err(|error| MError::internal(error.to_string()))
    }
}

impl ToolExecutor for MonopolyTools {
    fn execute(&self, call: ToolCall) -> BoxFuture<'_, MResult<String>> {
        Box::pin(async move { self.execute_inner(&call) })
    }
}

#[derive(Debug, Default, Deserialize)]
struct BoardSpaceArgs {
    #[serde(default)]
    position: Option<usize>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ListPropertiesArgs {
    #[serde(default)]
    color: Option<String>,
}

fn normalize_label(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{MonopolyTools, normalize_label};
    use crate::llm::{ToolCall, ToolExecutor};
    use crate::schemas::game::Edition;

    fn tools() -> MonopolyTools {
        MonopolyTools::new(
            serde_json::from_str(include_str!("../../public/uk_version.json")).unwrap(),
        )
    }

    fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "call_1".to_string(),
            name: name.to_string(),
            arguments,
        }
    }

    #[tokio::test]
    async fn looks_up_a_space_by_name() {
        let tools = tools();
        let result = tools
            .execute(call("get_board_space", json!({ "name": "old kent road" })))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["position"], 1);
        assert_eq!(parsed["space"]["name"], "Old Kent Road");
    }

    #[tokio::test]
    async fn lists_properties_filtered_by_colour() {
        let tools = tools();
        let result = tools
            .execute(call("list_properties", json!({ "color": "dark blue" })))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let properties = parsed["properties"].as_array().unwrap();

        assert_eq!(properties.len(), 2);
        assert!(properties.iter().all(|space| space["color"] == "DarkBlue"));
    }

    #[test]
    fn normalizes_human_colour_labels() {
        assert_eq!(normalize_label("Light Red"), "lightred");
        assert_eq!(normalize_label("dark-blue"), "darkblue");
    }

    #[test]
    fn definitions_are_non_empty() {
        let definitions = MonopolyTools::definitions();
        assert_eq!(definitions.len(), 2);
        assert!(definitions.iter().all(|tool| !tool.name.is_empty()));
        let _: Edition =
            serde_json::from_str(include_str!("../../public/uk_version.json")).unwrap();
    }
}
