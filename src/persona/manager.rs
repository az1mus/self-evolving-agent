/// Persona Manager - handles loading, registering, and retrieving personas
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::persona::{Persona, PersonaConfig, PersonaType};

/// Default persona directory
const DEFAULT_PERSONA_DIR: &str = ".sea/personas";

/// Manages persona configurations and instances
pub struct PersonaManager {
    /// Persona configurations directory
    persona_dir: PathBuf,
    /// Registered personas (name -> Persona)
    personas: HashMap<String, Persona>,
    /// Persona configurations (name -> config) - for lazy loading
    configs: HashMap<String, PersonaConfig>,
    /// Cache of loaded persona files
    loaded_files: HashMap<String, u64>, // file_path -> modified_time
}

impl PersonaManager {
    /// Create a new PersonaManager with default directory
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let persona_dir = home.join(DEFAULT_PERSONA_DIR);
        
        Self::with_dir(persona_dir)
    }
    
    /// Create a PersonaManager with custom directory
    pub fn with_dir(persona_dir: PathBuf) -> Result<Self> {
        // Create persona directory if it doesn't exist
        if !persona_dir.exists() {
            fs::create_dir_all(&persona_dir)
                .with_context(|| format!("Failed to create persona directory: {:?}", persona_dir))?;
        }
        
        let mut manager = Self {
            persona_dir,
            personas: HashMap::new(),
            configs: HashMap::new(),
            loaded_files: HashMap::new(),
        };
        
        // Load built-in personas
        manager.load_builtins()?;
        
        // Load user personas from disk
        manager.load_user_personas()?;

        Ok(manager)
    }

    /// Load built-in personas
    fn load_builtins(&mut self) -> Result<()> {
        log::info!("Loading built-in personas...");

        // Load built-in personas directly from hard-coded JSON strings
        for name in super::builtins::get_builtin_persona_names() {
            let content = super::builtins::get_builtin_persona_content(name)
                .with_context(|| format!("Failed to get built-in persona: {}", name))?;
            
            let config: PersonaConfig = serde_json::from_str(content)
                .with_context(|| format!("Failed to parse persona config: {}", name))?;
            
            let persona_name = config.name.clone();
            let persona = Persona::new(config.clone())?;
            
            self.personas.insert(persona_name.clone(), persona);
            self.configs.insert(persona_name.clone(), config);
            
            log::debug!("Loaded built-in persona: {}", persona_name);
        }

        log::info!("Loaded {} built-in personas", super::builtins::get_builtin_persona_names().len());
        Ok(())
    }
    
    /// Load user personas from disk
    pub fn load_user_personas(&mut self) -> Result<()> {
        log::info!("Loading user personas from: {:?}", self.persona_dir);

        if !self.persona_dir.exists() {
            return Ok(());
        }

        let builtin_names = super::builtins::get_builtin_persona_names();
        let mut count = 0;
        for entry in fs::read_dir(&self.persona_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                // Skip built-in personas (they are loaded from hard-coded strings)
                if let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) {
                    if builtin_names.contains(&file_name) {
                        log::debug!("Skipping built-in persona file: {:?}", path);
                        continue;
                    }
                }

                match self.load_persona_file(&path) {
                    Ok(_) => count += 1,
                    Err(e) => log::warn!("Failed to load persona {:?}: {}", path, e),
                }
            }
        }

        log::info!("Loaded {} user personas", count);
        Ok(())
    }
    
    /// Load a single persona file
    fn load_persona_file(&mut self, path: &Path) -> Result<()> {
        // Check if file has been modified
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        let path_str = path.to_string_lossy().to_string();
        
        // Skip if already loaded and not modified
        if let Some(&last_modified) = self.loaded_files.get(&path_str) {
            if last_modified == modified {
                return Ok(());
            }
        }
        
        // Load and parse
        let config = PersonaConfig::from_file(&path_str)?;
        let name = config.name.clone();
        
        // Validate and create persona
        let persona = Persona::new(config.clone())?;
        
        // Register (override if exists)
        self.personas.insert(name.clone(), persona);
        self.configs.insert(name.clone(), config);
        self.loaded_files.insert(path_str, modified);
        
        log::debug!("Loaded persona: {}", name);
        Ok(())
    }
    
    /// Reload all personas (useful for development)
    pub fn reload(&mut self) -> Result<()> {
        self.personas.clear();
        self.configs.clear();
        self.loaded_files.clear();
        
        self.load_builtins()?;
        self.load_user_personas()?;
        
        Ok(())
    }
    
    /// Register a persona from config
    pub fn register_persona(&mut self, config: PersonaConfig) -> Result<()> {
        let name = config.name.clone();
        let persona = Persona::new(config.clone())?;
        
        self.personas.insert(name.clone(), persona);
        self.configs.insert(name.clone(), config);
        
        log::info!("Registered persona: {}", name);
        Ok(())
    }
    
    /// Get a persona by name
    pub fn get_persona(&self, name: &str) -> Option<&Persona> {
        self.personas.get(name)
    }
    
    /// Get a persona config by name
    pub fn get_config(&self, name: &str) -> Option<&PersonaConfig> {
        self.configs.get(name)
    }

    /// Get all persona names
    pub fn list_personas(&self) -> Vec<&str> {
        self.personas.keys().map(|s| s.as_str()).collect()
    }

    /// Save a persona to file
    pub fn save_persona(&self, name: &str, path: Option<&str>) -> Result<()> {
        let config = self.configs.get(name)
            .with_context(|| format!("Persona not found: {}", name))?;

        let save_path = if let Some(p) = path {
            p.to_string()
        } else {
            format!("{}/{}.json", self.persona_dir.display(), name)
        };

        config.to_file(&save_path)?;
        log::info!("Saved persona '{}' to {}", name, save_path);
        Ok(())
    }

    /// Create a persona from template
    pub fn create_from_template(&mut self, name: &str, template: &str) -> Result<()> {
        let json_content = super::builtins::create_persona_from_template(name, template)?;
        let config: super::persona::PersonaConfig = serde_json::from_str(&json_content)?;
        self.register_persona(config)?;
        self.save_persona(name, None)?;
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> PersonaStats {
        PersonaStats {
            total: self.personas.len(),
            core: self.personas.values()
                .filter(|p| p.config.persona_type == PersonaType::Core)
                .count(),
            utility: self.personas.values()
                .filter(|p| p.config.persona_type == PersonaType::Utility)
                .count(),
            special: self.personas.values()
                .filter(|p| p.config.persona_type == PersonaType::Special)
                .count(),
        }
    }
}

impl Default for PersonaManager {
    fn default() -> Self {
        Self::new().expect("Failed to create PersonaManager")
    }
}

/// Statistics about personas
#[derive(Debug, Clone)]
pub struct PersonaStats {
    pub total: usize,
    pub core: usize,
    pub utility: usize,
    pub special: usize,
}

impl std::fmt::Display for PersonaStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Total: {}, Core: {}, Utility: {}, Special: {}", 
            self.total, self.core, self.utility, self.special)
    }
}
