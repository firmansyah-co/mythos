//! A project's HMI screens: the drafts engineers edit, and the set last released to operators.
//!
//! Stored as `screens/draft/screens.json` and `screens/active/screens.json` in the module's data
//! directory for the project. The same JSON the core stored before the HMI became a module.

use rand::random;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screen {
    pub id: String,
    pub name: String,
    pub is_root: bool,
    #[serde(default)]
    pub subscreens: Vec<SubScreen>,
    #[serde(rename = "canvasData")]
    pub canvas_data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubScreen {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

/// Which set of screens: what is being edited, or what operators see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Draft,
    Active,
}

impl Stage {
    fn dir(self) -> &'static str {
        match self {
            Stage::Draft => "draft",
            Stage::Active => "active",
        }
    }

    /// `?view=draft|active`; absent is the draft.
    pub fn from_view(view: Option<&str>) -> Result<Stage, String> {
        match view {
            None | Some("draft") => Ok(Stage::Draft),
            Some("active") => Ok(Stage::Active),
            Some(other) => Err(format!("unknown view '{other}': use 'draft' or 'active'")),
        }
    }
}

/// A failure, as the HTTP status and message the caller is answered with.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    pub status: u16,
    pub message: String,
}

fn refused(status: u16, message: impl ToString) -> Refused {
    Refused {
        status,
        message: message.to_string(),
    }
}

fn file(data: &Path, stage: Stage) -> std::path::PathBuf {
    data.join("screens").join(stage.dir()).join("screens.json")
}

pub fn read(data: &Path, stage: Stage) -> Result<Vec<Screen>, Refused> {
    match std::fs::read(file(data, stage)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            refused(
                500,
                format!("the {} screens cannot be read: {e}", stage.dir()),
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(refused(500, e)),
    }
}

fn write(data: &Path, stage: Stage, screens: &[Screen]) -> Result<(), Refused> {
    let bytes = serde_json::to_vec_pretty(screens).map_err(|e| refused(500, e))?;
    crate::write_atomic(&file(data, stage), &bytes).map_err(|e| refused(500, e))
}

pub fn create(data: &Path, name: &str, is_root: bool) -> Result<Screen, Refused> {
    let name = name.trim();
    if name.is_empty() {
        return Err(refused(400, "Screen name is required"));
    }
    let mut screens = read(data, Stage::Draft)?;
    let screen = Screen {
        id: format!("screen_{:032x}", random::<u128>()),
        name: name.to_owned(),
        is_root,
        subscreens: Vec::new(),
        canvas_data: json!({"title": name, "components": []}),
    };
    screens.push(screen.clone());
    write(data, Stage::Draft, &screens)?;
    Ok(screen)
}

/// Remove a screen from the draft. The released copy stays until the next release.
pub fn delete(data: &Path, id: &str) -> Result<(), Refused> {
    let mut screens = read(data, Stage::Draft)?;
    let before = screens.len();
    screens.retain(|s| s.id != id);
    if screens.len() == before {
        return Err(refused(404, "Screen not found"));
    }
    write(data, Stage::Draft, &screens)
}

pub fn create_subscreen(
    data: &Path,
    id: &str,
    name: &str,
    kind: &str,
) -> Result<SubScreen, Refused> {
    if name.trim().is_empty() || !["POPUP", "SLIDE_OVER", "EMBEDDED"].contains(&kind) {
        return Err(refused(400, "Invalid sub-screen name or type"));
    }
    let mut screens = read(data, Stage::Draft)?;
    let target = screens
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| refused(404, "Screen not found"))?;
    let sub = SubScreen {
        id: format!("sub_{:032x}", random::<u128>()),
        name: name.trim().to_owned(),
        kind: kind.to_owned(),
    };
    target.subscreens.push(sub.clone());
    write(data, Stage::Draft, &screens)?;
    Ok(sub)
}

pub fn save_draft(data: &Path, id: &str, canvas: Value) -> Result<(), Refused> {
    if !canvas.is_object() || !canvas.get("components").is_some_and(Value::is_array) {
        return Err(refused(400, "Canvas must contain a components array"));
    }
    let mut screens = read(data, Stage::Draft)?;
    let target = screens
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| refused(404, "Screen not found"))?;
    target.canvas_data = canvas;
    write(data, Stage::Draft, &screens)
}

/// Release the draft: it becomes what operators see. Returns what was released.
pub fn release(data: &Path) -> Result<Vec<Screen>, Refused> {
    let screens = read(data, Stage::Draft)?;
    write(data, Stage::Active, &screens)?;
    Ok(screens)
}
