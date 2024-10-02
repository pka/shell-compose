use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use thiserror::Error;

pub struct Justfile {
    justfile: JustfileDump,
}

/// Output of `just --dump --dump-format json`
#[derive(Deserialize, Debug)]
struct JustfileDump {
    // aliases: {},
    // assignments: {},
    // doc: null,
    // first: Option<String>,
    // groups: [],
    // modules: {},
    recipes: HashMap<String, JustfileRecipe>,
    // settings: HashMap<String, serde_json::Value>,
    //   "allow_duplicate_recipes": false,
    //   "allow_duplicate_variables": false,
    //   "dotenv_filename": null,
    //   "dotenv_load": false,
    //   "dotenv_path": null,
    //   "dotenv_required": false,
    //   "export": false,
    //   "fallback": false,
    //   "ignore_comments": false,
    //   "positional_arguments": false,
    //   "quiet": false,
    //   "shell": null,
    //   "tempdir": null,
    //   "unstable": false,
    //   "windows_powershell": false,
    //   "windows_shell": null,
    //   "working_directory": null
    //  unexports: [],
    //  warnings: []
}

#[derive(Deserialize, Debug)]
struct JustfileRecipe {
    attributes: Vec<HashMap<String, String>>,
    //   "group": "autostart"
    // body: [...],
    // dependencies: [],
    // doc: null,
    name: String,
    // namepath: String,
    // parameters: [],
    // priors: 0,
    // private: false,
    // quiet: false,
    // shebang: true
}

#[derive(Error, Debug)]
pub enum JustfileError {
    #[error("Error in calling just executable: {0}")]
    SpawnError(#[from] std::io::Error),
    #[error("Invalid characters in justfile: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),
    #[error("justfile version mismatch: {0}")]
    JsonError(#[from] serde_json::error::Error),
}

impl Justfile {
    pub fn parse() -> Result<Self, JustfileError> {
        let output = Command::new("just")
            .args(["--dump", "--dump-format", "json"])
            .output()?;
        let jsonstr = String::from_utf8(output.stdout)?;
        let justfile = serde_json::from_str(&jsonstr)?;
        let just = Justfile { justfile };
        Ok(just)
    }
    pub fn group_recipes(&self, group: &str) -> Vec<String> {
        let recipes = self.justfile.recipes.values().filter(|recipe| {
            recipe
                .attributes
                .iter()
                .any(|attr| attr.get("group").map(|g| g == group).unwrap_or(false))
        });
        recipes.map(|recipe| recipe.name.clone()).collect()
    }
}
