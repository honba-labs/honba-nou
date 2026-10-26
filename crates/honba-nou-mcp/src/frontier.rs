//! Frontier Agent Integration (Task 3.3).
//!
//! Generates Model Context Protocol configuration manifests for:
//! - Claude Desktop (`claude_desktop_config.json`)
//! - Cursor (`cursor_mcp.json` / `.cursor/mcp.json`)
//! - Gemini CLI (`gemini_mcp.json`)

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Claude Desktop configuration format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeDesktopConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: BTreeMap<String, ClaudeServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeServerEntry {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// Cursor configuration format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CursorMcpConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: BTreeMap<String, CursorServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CursorServerEntry {
    Stdio {
        command: String,
        args: Vec<String>,
        #[serde(default = "default_stdio_transport")]
        transport: String,
    },
    Sse {
        url: String,
        transport: String,
    },
}

fn default_stdio_transport() -> String {
    "stdio".to_string()
}

/// Gemini CLI configuration format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeminiMcpConfig {
    pub mcp_servers: BTreeMap<String, GeminiServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeminiServerEntry {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// Collection of generated manifest file paths.
#[derive(Debug, Clone)]
pub struct AgentManifestPaths {
    pub claude_desktop_path: PathBuf,
    pub cursor_path: PathBuf,
    pub gemini_path: PathBuf,
}

/// Generator for Frontier Agent MCP manifests.
pub struct ManifestGenerator;

impl ManifestGenerator {
    /// Generate Claude Desktop configuration JSON for honba-nou.
    pub fn generate_claude_desktop(command: &str, args: &[&str]) -> ClaudeDesktopConfig {
        let mut servers = BTreeMap::new();
        let mut env = BTreeMap::new();
        env.insert("RUST_LOG".to_string(), "info".to_string());
        env.insert("HONBA_ENV".to_string(), "production".to_string());

        servers.insert(
            "honba-nou".to_string(),
            ClaudeServerEntry {
                command: command.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
                env,
            },
        );

        ClaudeDesktopConfig {
            mcp_servers: servers,
        }
    }

    /// Generate Cursor configuration JSON for honba-nou (supporting stdio or SSE).
    pub fn generate_cursor(command: &str, args: &[&str], sse_url: Option<&str>) -> CursorMcpConfig {
        let mut servers = BTreeMap::new();

        let entry = if let Some(url) = sse_url {
            CursorServerEntry::Sse {
                url: url.to_string(),
                transport: "sse".to_string(),
            }
        } else {
            CursorServerEntry::Stdio {
                command: command.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
                transport: "stdio".to_string(),
            }
        };

        servers.insert("honba-nou".to_string(), entry);

        CursorMcpConfig {
            mcp_servers: servers,
        }
    }

    /// Generate Gemini CLI configuration JSON for honba-nou.
    pub fn generate_gemini_cli(command: &str, args: &[&str]) -> GeminiMcpConfig {
        let mut servers = BTreeMap::new();
        let mut env = BTreeMap::new();
        env.insert("HONBA_SEBI_COMPLIANCE".to_string(), "strict".to_string());

        servers.insert(
            "honba-nou".to_string(),
            GeminiServerEntry {
                command: command.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
                env,
            },
        );

        GeminiMcpConfig {
            mcp_servers: servers,
        }
    }

    /// Write all manifests to a designated output directory.
    pub fn write_all(
        output_dir: &Path,
        command: &str,
        args: &[&str],
        sse_url: Option<&str>,
    ) -> std::io::Result<AgentManifestPaths> {
        fs::create_dir_all(output_dir)?;

        let claude_cfg = Self::generate_claude_desktop(command, args);
        let cursor_cfg = Self::generate_cursor(command, args, sse_url);
        let gemini_cfg = Self::generate_gemini_cli(command, args);

        let claude_path = output_dir.join("claude_desktop_config.json");
        let cursor_path = output_dir.join("cursor_mcp.json");
        let gemini_path = output_dir.join("gemini_mcp.json");

        fs::write(&claude_path, serde_json::to_string_pretty(&claude_cfg)?)?;
        fs::write(&cursor_path, serde_json::to_string_pretty(&cursor_cfg)?)?;
        fs::write(&gemini_path, serde_json::to_string_pretty(&gemini_cfg)?)?;

        Ok(AgentManifestPaths {
            claude_desktop_path: claude_path,
            cursor_path,
            gemini_path,
        })
    }
}
