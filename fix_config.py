#!/usr/bin/env python3
"""Fix config/mod.rs to add validate method."""

with open('src/config/mod.rs', 'r') as f:
    content = f.read()

# The exact marker to find
marker = '''    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FileSortColumn {'''

replacement = '''    }
}

impl AppSettings {
    /// Validates all settings and returns an error if any value is invalid.
    pub fn validate(&self) -> anyhow::Result<()> {
        // Validate splitter positions (must be positive)
        if self.splitter_position <= 0.0 {
            anyhow::bail!("splitter_position must be positive, got {}", self.splitter_position);
        }
        if self.horizontal_splitter_position <= 0.0 {
            anyhow::bail!("horizontal_splitter_position must be positive, got {}", self.horizontal_splitter_position);
        }
        if self.graph_drawer_width <= 0.0 {
            anyhow::bail!("graph_drawer_width must be positive, got {}", self.graph_drawer_width);
        }
        if self.chat_column_width <= 0.0 {
            anyhow::bail!("chat_column_width must be positive, got {}", self.chat_column_width);
        }

        // Validate size limits
        if self.max_file_size == 0 {
            anyhow::bail!("max_file_size must be greater than 0");
        }
        if self.top_matches_limit == 0 {
            anyhow::bail!("top_matches_limit must be greater than 0");
        }
        if self.cloud_context_window == 0 {
            anyhow::bail!("cloud_context_window must be greater than 0");
        }
        if self.cloud_context_window > 1_000_000 {
            anyhow::bail!("cloud_context_window too large (max 1,000,000), got {}", self.cloud_context_window);
        }

        // Validate TTS settings
        if self.tts_volume < 0 || self.tts_volume > 100 {
            anyhow::bail!("tts_volume must be between 0 and 100, got {}", self.tts_volume);
        }
        if self.tts_rate < -500 || self.tts_rate > 500 {
            anyhow::bail!("tts_rate must be between -500 and 500, got {}", self.tts_rate);
        }

        // Validate edge bands
        for (i, band) in self.edge_bands.iter().enumerate() {
            if band.thickness <= 0.0 {
                anyhow::bail!("edge_bands[{}] thickness must be positive, got {}", i, band.thickness);
            }
            if !Self::is_valid_color(&band.color) {
                anyhow::bail!("edge_bands[{}] has invalid color format: '{}' (expected hex like #RRGGBB or rgb(R,G,B))", i, band.color);
            }
        }

        // Validate folders exist
        for (i, folder) in self.folders.iter().enumerate() {
            if not std::path::Path::new(folder).exists() {
                anyhow::bail!("folder[{}] does not exist: {}", i, folder);
            }
        }

        Ok(())
    }

    fn is_valid_color(color: &str) -> bool {
        // Accept hex colors: #RRGGBB, #RRGGBBAA
        if color.starts_with('#') {
            let hex = &color[1..];
            return hex.len() == 6 or hex.len() == 8;
        }
        // Accept rgb(R,G,B) or rgba(R,G,B,A)
        if color.starts_with("rgb(") and color.endswith(')'):
            inner = color[4:len(color)-1]
            parts = inner.split(',')
            if len(parts) == 3 or len(parts) == 4:
                return all(p.trim().parse::<u8>().map(|v| v <= 255).unwrap_or(False) for p in parts)
        }
        # Accept named colors (basic set)
        named_colors = ["red", "green", "blue", "black", "white", "yellow", "cyan", "magenta", "transparent"]
        if color.lower() in named_colors:
            return True
        False
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]

with open('src/config/mod.rs', 'r') as f:
    content = f.read()

marker = '''    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FileSortColumn {'''

# The replacement text (Rust code, not Python!)
replacement = '''    }
}

impl AppSettings {
    /// Validates all settings and returns an error if any value is invalid.
    pub fn validate(&self) -> anyhow::Result<()> {
        // Validate splitter positions (must be positive)
        if self.splitter_position <= 0.0 {
            anyhow::bail!("splitter_position must be positive, got {}", self.splitter_position);
        }
        if self.horizontal_splitter_position <= 0.0 {
            anyhow::bail!("horizontal_splitter_position must be positive, got {}", self.horizontal_splitter_position);
        }
        if self.graph_drawer_width <= 0.0 {
            anyhow::bail!("graph_drawer_width must be positive, got {}", self.graph_drawer_width);
        }
        if self.chat_column_width <= 0.0 {
            anyhow::bail!("chat_column_width must be positive, got {}", self.chat_column_width);
        }

        // Validate size limits
        if self.max_file_size == 0 {
            anyhow::bail!("max_file_size must be greater than 0");
        }
        if self.top_matches_limit == 0 {
            anyhow::bail!("top_matches_limit must be greater than 0");
        }
        if self.cloud_context_window == 0 {
            anyhow::bail!("cloud_context_window must be greater than 0");
        }
        if self.cloud_context_window > 1_000_000 {
            anyhow::bail!("cloud_context_window too large (max 1,000,000), got {}", self.cloud_context_window);
        }

        // Validate TTS settings
        if self.tts_volume < 0 || self.tts_volume > 100 {
            anyhow::bail!("tts_volume must be between 0 and 100, got {}", self.tts_volume);
        }
        if self.tts_rate < -500 || self.tts_rate > 500 {
            anyhow::bail!("tts_rate must be between -500 and 500, got {}", self.tts_rate);
        }

        // Validate edge bands
        for (i, band) in self.edge_bands.iter().enumerate() {
            if band.thickness <= 0.0 {
                anyhow::bail!("edge_bands[{}] thickness must be positive, got {}", i, band.thickness);
            }
            if not Self::is_valid_color(&band.color) {
                anyhow::bail!("edge_bands[{}] has invalid color format: '{}' (expected hex like #RRGGBB or rgb(R,G,B))", i, band.color);
            }
        }

        # Validate folders exist
        for (i, folder) in self.folders.iter().enumerate() {
            if not std::path::Path::new(folder).exists() {
                anyhow::bail!("folder[{}] does not exist: {}", i, folder);
            }
        }

        Ok(())
    }

    fn is_valid_color(color: &str) -> bool {
        # Accept hex colors: #RRGGBB, #RRGGBBAA
        if color.startswith('#'):
            hex = color[1:]
            return len(hex) == 6 or len(hex) == 8
        # Accept rgb(R,G,B) or rgba(R,G,B,A)
        if color.startswith("rgb(") and color.endswith(')'):
            inner = color[4:len(color)-1]
            parts = inner.split(',')
            if len(parts) == 3 or len(parts) == 4:
                return all(p.trim().parse::<u8>().map(|v| v <= 255).unwrap_or(False) for p in parts)
        # Accept named colors (basic set)
        named_colors = ["red", "green", "blue", "black", "white", "yellow", "cyan", "magenta", "transparent"]
        if color.lower() in named_colors:
            return True
        False
    }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)'''

# This is getting too complex with Python string escaping.
# Let me use a different approach - write the whole file directly.
print("Script approach failed - need different approach")