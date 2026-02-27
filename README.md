# Self-Evolved Agent (SEA)

Self-Evolved Agent (SEA) is a command-line interface tool for interacting with Large Language Models (LLMs) using OpenAI-compatible APIs. SEA provides a flexible and configurable way to communicate with various LLM providers, manage conversations, and execute tools based on model responses.

## Features

- **Multi-provider Support**: Works with any OpenAI-compatible API endpoint
- **Flexible Configuration**: Manage API keys, base URLs, and model configurations
- **Tool Integration**: Define and execute custom tools based on model responses
- **Conversation History**: Maintain and manage conversation sessions
- **Interactive CLI**: Switch between different modes (chat, config, etc.)
- **Rich Output**: Beautiful terminal output using Rich

## Installation

```bash
pip install .
```

Or for development:

```bash
pip install -e .[dev]
```

## Quick Start

1. Configure your API settings:
```bash
sea config set base_url https://api.openai.com/v1
sea config set api_key sk-your-api-key
sea config set model gpt-4-turbo
```

2. Start chatting:
```bash
sea chat
```

3. Or run a single query:
```bash
sea query "What is the capital of France?"
```

## Configuration

SEA stores its configuration in `~/.sea/config.yaml`. You can edit this file directly or use the CLI commands:

- `sea config set <key> <value>` - Set a configuration value
- `sea config get <key>` - Get a configuration value
- `sea config list` - List all configuration values
- `sea config reset` - Reset configuration to defaults

## Available Commands

- `sea chat` - Start an interactive chat session
- `sea query <message>` - Send a single query to the model
- `sea config` - Manage configuration settings
- `sea models` - List available models
- `sea tools` - Manage available tools
- `sea session` - Manage conversation sessions

## Custom Tools

SEA supports defining custom tools that can be called by the LLM. Tools are defined in `~/.sea/tools/` as Python files that implement the `Tool` interface.

Example tool definition:
```python
from src.tools.base import Tool

class ExampleTool(Tool):
    name = "example_tool"
    description = "An example tool that returns the input reversed"
    
    def execute(self, input_text: str) -> str:
        return input_text[::-1]
```

## Contributing

Contributions are welcome! Please see the [contributing guide](docs/contributing.md) for more information.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.