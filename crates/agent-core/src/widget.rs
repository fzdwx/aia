use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiWidgetPhase {
    Preview,
    Final,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UiWidgetDocument {
    pub title: String,
    pub description: String,
    pub html: String,
    pub content_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UiWidget {
    pub instance_id: String,
    pub phase: UiWidgetPhase,
    pub document: UiWidgetDocument,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WidgetCanvasSnapshot {
    pub key: String,
    pub data_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetHostCommand {
    Render { widget: UiWidget },
    ThemeTokens { tokens: BTreeMap<String, String> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetClientEvent {
    Ready,
    ScriptsReady,
    Resize {
        height: u32,
        first: bool,
    },
    Error {
        message: String,
    },
    SendPrompt {
        text: String,
    },
    OpenLink {
        href: String,
    },
    Captured {
        html: Option<String>,
        styles: Option<String>,
        canvases: Vec<WidgetCanvasSnapshot>,
        body_width: u32,
        body_height: u32,
    },
}
