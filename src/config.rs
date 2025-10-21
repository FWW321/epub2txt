use std::sync::LazyLock;

use ahash::AHashSet;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::Deserializer;

static CONFIG: LazyLock<Config> = LazyLock::new(|| {
    Config::load().expect("Failed to load configuration")
});

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub output_dir: String,
    pub input_dir: String,
    pub separator: String,
    pub tags: Tags,
    pub options: Options,
}

impl Config {
    pub fn load() -> Result<Self> {
        config::Config::builder()
            .add_source(
                config::File::with_name("config")
                    .format(config::FileFormat::Toml)
                    .required(false),
            )
            .build()?
            .try_deserialize()
            .with_context(|| anyhow::anyhow!("Failed to load config"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_dir: "output".to_string(),
            input_dir: "input".to_string(),
            separator: "".to_string(),
            tags: Tags::default(),
            options: Options::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Options {
    pub split: bool,
    pub combine: bool,
    pub metadata: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            split: true,
            combine: true,
            metadata: true,
        }
    }
}

#[derive(Debug)]
pub struct Tags {
    pub title: AHashSet<Vec<u8>>,
    pub block: AHashSet<Vec<u8>>,
    pub inline: AHashSet<Vec<u8>>,
}

impl Default for Tags {
    fn default() -> Self {
        let raw_tags = RawTags::default();
        Self::from(raw_tags)
    }
}

impl<'de> Deserialize<'de> for Tags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawTags::deserialize(deserializer)?;

        Ok(Self::from(raw))
    }
}

impl From<RawTags> for Tags {
    fn from(raw: RawTags) -> Self {
        Self {
            title: raw.title.into_iter().map(|s| s.into_bytes()).collect(),
            block: raw.block.into_iter().map(|s| s.into_bytes()).collect(),
            inline: raw.inline.into_iter().map(|s| s.into_bytes()).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawTags {
    title: Vec<String>,
    block: Vec<String>,
    inline: Vec<String>,
}

impl Default for RawTags {
    fn default() -> Self {
        Self {
            title: vec!["h1".to_string(), "title".to_string()],
            block: vec![
                "p".to_string(),
                "div".to_string(),
                "li".to_string(),
                "ul".to_string(),
                "section".to_string(),
                "br".to_string(),
            ],
            inline: vec![
                "em".to_string(),
                "span".to_string(),
                "a".to_string(),
                "strong".to_string(),
                "em".to_string(),
                "code".to_string(),
                "sub".to_string(),
                "sup".to_string(),
            ],
        }
    }
}

pub fn get_config() -> &'static Config {
    &CONFIG
}
