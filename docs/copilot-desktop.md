# GitHub Copilot Desktop adapter

`rai sync` projects the canonical `.agents/` resources into the formats discovered by GitHub Copilot in desktop VS Code.

| Canonical resource | Copilot Desktop projection |
| --- | --- |
| `.agents/rules/*.md` | Global rules become `.github/copilot-instructions.md`; scoped rules become `.github/instructions/*.instructions.md` with `applyTo`. |
| `.agents/subagents/<name>.md` | `.github/agents/<name>.agent.md` with `target: vscode`; the Markdown body contains `developer_instructions`. |
| `.agents/skills/<name>/SKILL.md` | Read directly in place by VS Code; validated, not copied. |
| `.agents/mcp.yaml` | `.vscode/mcp.json`, including environment-variable references for credentials. |

Copilot model names and Codex model names are not interchangeable. RosettAI therefore leaves the model selected in VS Code and does not copy the Codex-specific `model`, `model_reasoning_effort`, `sandbox_mode`, or `nickname_candidates` fields into the Copilot agent. The portable role, description, and developer instructions are preserved.

Generated files are local projections and are added to `.gitignore`. RosettAI only updates or removes projections whose integrity marker still matches. Existing user-owned, modified, or Git-tracked native files cause a conflict instead of being overwritten.

To verify discovery in VS Code, open the repository, open Copilot Chat, right-click the Chat view, and select **Diagnostics**. The diagnostics view lists loaded instruction files, custom agents, and skills, with parse errors. The custom agent also appears in the agent picker. MCP servers appear under **MCP: List Servers**. Reload the window or start a fresh chat after synchronization.

References: [VS Code custom agents](https://code.visualstudio.com/docs/agent-customization/custom-agents), [custom instructions](https://code.visualstudio.com/docs/agent-customization/custom-instructions), and [MCP configuration](https://code.visualstudio.com/docs/agents/reference/mcp-configuration).
