# AI coding-agent harnesses

Last reviewed: 2026-09-24.

This document inventories developer-facing AI agent harnesses: applications that
can inspect a codebase, plan work, edit files, and usually run commands or tests.
It is a discovery list for possible RosettAI adapters, not a statement that
RosettAI currently supports every entry.

No list can prove that it contains every private, experimental, or newly released
agent. The catalog therefore aims to cover publicly documented harnesses with a
usable product or source repository. A linked primary source is required for each
entry. Products that only provide completion or chat, and agent-building SDKs
without a coding-agent user experience, are excluded.

## Can RosettAI synchronize before the LLM call?

There are two different guarantees:

- **Full projection** means rules, agents, skills, MCP declarations, hooks, and
  other configuration are generated before the harness reads its configuration.
  For a local executable, `rai run <harness>` can provide this guarantee by
  synchronizing first and launching the harness only after success.
- **Per-prompt refresh** means a native synchronous hook can run `rai sync`
  after the user submits a prompt but before that turn reaches the model. This
  is useful for long-running sessions, but it is generally too late to replace
  configuration that the harness loaded at process or session startup. Where
  supported, the hook should also return or inject the freshly generated rules
  if the harness does not reload rule files for each turn.

The classifications below use these values:

- **Yes** — a documented synchronous pre-prompt or pre-model command hook exists.
- **Conditional** — the hook exists only on some surfaces, or requires `rai` and
  its script to be installed in a hosted execution environment.
- **Wrapper only** — no qualifying native hook is documented, but a local CLI
  can be launched safely through `rai run`.
- **No** — the managed surface exposes neither a qualifying hook nor a launch
  boundary controlled by the user.
- **Source change** — possible only by modifying or embedding the open-source
  runner; this is not a stable user-facing hook contract.

Negative findings mean "not present in the linked public documentation as of the
review date," not proof that a private or experimental API does not exist.

### Local and terminal-first harnesses

| Harness | Full projection before first LLM call | Native per-prompt refresh | Recommended RosettAI integration |
| --- | --- | --- | --- |
| Aider | Yes, wrapper | Wrapper only | Run `rai sync` and then `aider`; no documented pre-prompt lifecycle command hook. |
| Amp | Yes, wrapper | Yes | Use an [`agent.start` plugin](https://ampcode.com/docs/customize/plugins) for rules; retain the wrapper for startup-loaded configuration. |
| Claude Code | Yes, wrapper | Yes | Use [`UserPromptSubmit`](https://docs.anthropic.com/en/docs/claude-code/hooks#userpromptsubmit); emit `additionalContext` for same-turn rules. |
| Codex | Yes, wrapper | Yes locally; conditional in hosted environments | Use [`UserPromptSubmit`](https://developers.openai.com/de-DE/docs/hooks); hosted hooks require the script to exist and be trusted in the execution environment. |
| Continue | Yes, wrapper | Wrapper only | Wrap `cn`; the documented customization surface does not expose a synchronous shell hook before each prompt. |
| Crush | Yes, wrapper | Wrapper only | Wrap `crush`; no documented pre-prompt command hook. |
| Droid | Yes, wrapper | Yes | Use [`UserPromptSubmit`](https://docs.factory.ai/harness/hooks) and return `additionalContext` when rules must affect the current turn. |
| Gemini CLI | Yes, wrapper | Yes | Use [`BeforeModel`](https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/reference.md) for every model request, or `BeforeAgent` once per user turn. |
| GitHub Copilot CLI | Yes, wrapper | Yes, with a limitation | Use [`userPromptSubmitted`](https://docs.github.com/en/copilot/reference/hooks-reference) to run `rai`; command-hook output is discarded, so rely on files the runtime reloads or the SDK hook for context injection. |
| Goose | Yes, wrapper | Wrapper only | Wrap `goose`; recipes and extensions are not a documented synchronous pre-model shell-hook boundary. |
| Junie CLI | Yes, wrapper | Wrapper only | Wrap `junie`; guidelines, skills, commands, and MCP are documented, but no pre-prompt command hook is. |
| Kiro CLI | Yes, wrapper | Yes | Use a [Prompt Submit shell-command hook](https://kiro.dev/docs/hooks/types/); successful stdout is added to agent context. |
| Mentat | Yes, wrapper | No; archived | The old CLI can be wrapped, but no new adapter or hook integration should be prioritized. |
| OpenCode | Yes, wrapper | Yes | Use the [`prompt` session hook](https://opencode.ai/v2/docs/build/plugins); use the model-call hook instead if synchronization must precede retries and internal calls too. |
| OpenHands CLI | Yes, wrapper | Wrapper only | Wrap the CLI. The SDK can implement a custom boundary, but the stock CLI does not document a user-configured pre-prompt shell hook. |
| Open Interpreter | Yes, wrapper | Wrapper only | Wrap `interpreter`; no documented pre-prompt command hook. |
| Plandex | Yes, wrapper | Wrapper only | Wrap `plandex`; no documented pre-prompt command hook. |
| Qwen Code | Yes, wrapper | Yes | Use [`UserPromptSubmit`](https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/), which runs before supported model invocations. |
| Warp Agent | Yes for the local CLI | Wrapper only | Wrap the `warp` CLI. Warp rules/skills/MCP do not currently document a user-defined pre-prompt command hook for the agent. |

### IDE, editor, and desktop harnesses

| Harness | Full projection before first LLM call | Native per-prompt refresh | Recommended RosettAI integration |
| --- | --- | --- | --- |
| Augment Code | Setup/watch only | No | Synchronize before opening the workspace and keep a watcher running; no documented pre-prompt shell hook. |
| Cline | Setup/watch only | No | Generate before VS Code opens or use a companion watcher; workflows and MCP do not provide a deterministic pre-prompt command hook. |
| Cursor Agent | Setup/watch or CLI wrapper | Guard only | [`beforeSubmitPrompt`](https://cursor.com/docs/hooks) runs before the backend request, but Cursor does not guarantee that rules written by the hook are re-resolved for that same request. Use it to detect drift, synchronize, block the prompt, and ask the user to resubmit. |
| GitHub Copilot agent mode | Setup/watch only | No for the IDE surface | The documented command hooks target Copilot CLI and cloud agent, not ordinary IDE agent mode; use workspace setup/watch. |
| Google Antigravity | Setup/watch only | No | Use workspace setup/watch; no documented synchronous pre-prompt shell hook. |
| JetBrains Junie | Setup/watch only | No | Use IDE startup/project-open integration or a watcher; no documented pre-prompt command hook. |
| Kilo Code | Setup/watch or CLI wrapper | No | Wrap the CLI where used; for IDE use, synchronize on workspace setup/watch. No qualifying hook is documented. |
| Kiro IDE | Setup/watch only | Yes | Use a [Prompt Submit shell-command hook](https://kiro.dev/docs/hooks/actions/); retain project-open sync for startup-loaded configuration. |
| PearAI | Setup/watch only | No | Use editor startup/watch; no stable documented pre-prompt hook API. |
| Roo Code | Setup/watch only | No | Use workspace setup/watch; modes and MCP do not provide a documented synchronous pre-prompt command hook. |
| Tabnine Agent | Setup/watch only | No | Use workspace setup/watch; no documented user-defined pre-prompt command hook. |
| Trae | Setup/watch only | No | Use workspace setup/watch; no documented user-defined pre-prompt command hook. |
| Void | Setup/watch only | No | Use editor startup/watch; no stable documented pre-prompt hook API. |
| Windsurf Cascade | Setup/watch only | Yes | Use [`pre_user_prompt`](https://docs.windsurf.com/windsurf/cascade/hooks); retain workspace-open sync for startup-loaded configuration. |
| Zed Agent Panel | Setup/watch only | No | Zed task hooks are editor/task events, not pre-agent-prompt hooks; synchronize on project open or use a watcher. |

### Remote and asynchronous coding agents

| Harness | Full projection before first LLM call | Native per-prompt refresh | Recommended RosettAI integration |
| --- | --- | --- | --- |
| Claude Code on the web | Conditional | Conditional | Repository hooks can work only when the hosted environment installs `rai`, contains the hook script, and permits it; otherwise pre-generate committed portable instructions. |
| Codegen | No | No | Use provider-supported repository instructions; no documented customer command hook before the first model call. |
| Codex cloud | Conditional | Conditional | A Codex `UserPromptSubmit` hook is viable only when `rai` and the trusted hook script exist in the task environment; do not rely on web plugin installation to deploy scripts. |
| Cosine Genie | No | No | Use supported repository context/configuration; no documented customer pre-prompt command hook. |
| Cursor background agents | Conditional | Guard only | Project [`beforeSubmitPrompt` hooks](https://cursor.com/docs/hooks) can run in writable cloud-agent phases, but cannot inject context or guarantee same-turn rule re-resolution. Provision and run `rai` before the agent starts. |
| Devin | No | No | Use Devin knowledge/setup facilities; no documented customer hook immediately before model invocation. |
| GitHub Copilot cloud agent | Conditional | Yes, with a limitation | Repository [`userPromptSubmitted`](https://docs.github.com/en/copilot/concepts/agents/hooks) hooks run in the cloud environment, but command output cannot inject updated context; provision `rai` first. |
| Google Jules | No | No | Use repository instructions and environment setup; no documented customer pre-prompt command hook. |
| OpenHands Cloud | No | No | A custom deployment can wrap or modify the runner, but the managed service exposes no documented pre-prompt shell hook. |
| Qodo Merge | No | No | Use Qodo configuration/instructions; no documented customer shell hook before model invocation. |
| Replit Agent | No | No | Use Replit project configuration; no documented customer pre-prompt shell hook. |
| Sweep | No | No | Use repository configuration; the hosted agent exposes no documented pre-prompt shell hook. |

### Software-engineering agent runners and research harnesses

| Harness | Full projection before first LLM call | Native per-prompt refresh | Recommended RosettAI integration |
| --- | --- | --- | --- |
| Agentless | Yes, wrapper | Source change | Run it through a RosettAI wrapper; per-call interception requires modifying its model-call code. |
| AutoCodeRover | Yes, wrapper | Source change | Run it through a RosettAI wrapper; per-call interception requires modifying its runner. |
| Devika | Yes, wrapper | Source change | Wrap the self-hosted process; no stable user-facing pre-prompt hook contract is documented. |
| GPT Pilot | Yes, wrapper | Source change | Wrap the self-hosted process; per-call interception requires modifying or embedding the runner. |
| OpenHands | Yes, wrapper or custom deployment | Source change | Use a wrapper for the stock runner or implement synchronization in the OpenHands SDK conversation boundary. |
| SWE-agent | Yes, wrapper | Source change | Run through a wrapper; per-call synchronization requires a custom agent/model implementation rather than configuration. |

### Adapter policy implied by the survey

RosettAI adapters should implement the following order of preference:

1. Always synchronize the complete projection before launching a local harness.
2. Install a synchronous native prompt/model hook where one exists, but use it
   only for resources the running harness can reload or for context the hook can
   inject into the current turn.
3. For IDEs without prompt hooks, synchronize at workspace open and run a file
   watcher. Report that this is eventual consistency, not a pre-LLM guarantee.
4. For hosted agents, report **unmanaged** unless RosettAI can prove that the
   execution image contains `rai`, the hook is trusted, and synchronization
   completes before the first model request.
5. Fail closed when a qualifying pre-prompt hook cannot complete: a stale
   projection must not be silently presented as current.

For Cursor specifically, `beforeSubmitPrompt` is a drift guard rather than a
same-turn synchronization boundary. Its input already identifies attached rule
files, while its documented output supports only `continue` and `user_message`.
It cannot request a rule reload or add context. A safe hook therefore runs
`rai status`, and when stale, runs `rai sync` and returns `continue: false` with
an instruction to resubmit after synchronization. Normal operation should keep
the projection current before submission through workspace-open synchronization
and a watcher.

## Local and terminal-first harnesses

| Harness | Provider | Interface | Source | Notes |
| --- | --- | --- | --- | --- |
| Aider | Aider-AI | CLI | [Repository](https://github.com/Aider-AI/aider) | Open source; model-agnostic pair-programming agent. |
| Amp | Sourcegraph | CLI, editor | [Documentation](https://ampcode.com/manual) | Commercial agent with a terminal CLI and editor integrations. |
| Claude Code | Anthropic | CLI, IDE, desktop | [Documentation](https://docs.anthropic.com/en/docs/claude-code/overview) | Supports project instructions, subagents, skills, hooks, and MCP. |
| Codex | OpenAI | CLI, IDE, desktop, cloud | [Documentation](https://developers.openai.com/codex/) | Open-source CLI plus local and delegated cloud execution surfaces. |
| Continue | Continue | CLI, VS Code, JetBrains | [Documentation](https://docs.continue.dev/) | Open-source, configurable agents for terminal and IDE use. |
| Crush | Charmbracelet | CLI | [Repository](https://github.com/charmbracelet/crush) | Open-source, multi-model terminal coding agent. |
| Droid | Factory | CLI, IDE, web | [Documentation](https://docs.factory.ai/) | Commercial coding agent for interactive and delegated work. |
| Gemini CLI | Google | CLI | [Repository](https://github.com/google-gemini/gemini-cli) | Open-source terminal agent with project instructions and MCP. |
| GitHub Copilot CLI | GitHub | CLI | [Documentation](https://docs.github.com/en/copilot/concepts/agents/copilot-cli/about-copilot-cli) | Terminal-native Copilot agent; distinct from the older `gh copilot` command. |
| Goose | Block | CLI, desktop | [Documentation](https://block.github.io/goose/) | Open-source, extensible local agent with MCP support. |
| Junie CLI | JetBrains | CLI, IDE, CI | [Documentation](https://junie.jetbrains.com/docs/) | JetBrains agent for terminal, IDE, and headless execution. |
| Kiro CLI | AWS | CLI | [Documentation](https://kiro.dev/docs/cli/) | Current name of Amazon Q Developer CLI; supports agent resources and MCP. |
| Mentat | AbanteAI | CLI | [Archived repository](https://github.com/AbanteAI/archive-old-cli-mentat) | Archived open-source command-line coding assistant. |
| OpenCode | Anomaly | CLI, desktop, IDE | [Documentation](https://opencode.ai/docs/) | Open-source terminal-first agent with commands, agents, rules, and MCP. |
| OpenHands CLI | OpenHands | CLI | [Documentation](https://docs.openhands.dev/openhands/usage/run-openhands/local-setup) | Local interface backed by the OpenHands software-agent runtime. |
| Open Interpreter | Open Interpreter | CLI, desktop | [Repository](https://github.com/OpenInterpreter/open-interpreter) | General computer/code execution agent that can operate on local projects. |
| Plandex | Plandex AI | CLI | [Repository](https://github.com/plandex-ai/plandex) | Open-source terminal agent designed for large, multi-step changes. |
| Qwen Code | QwenLM | CLI | [Repository](https://github.com/QwenLM/qwen-code) | Open-source terminal agent derived from Gemini CLI concepts. |
| Warp Agent | Warp | Terminal, CLI, cloud | [Documentation](https://docs.warp.dev/agents/) | Agent available in Warp, as a CLI, and as a cloud agent. |

## IDE, editor, and desktop harnesses

| Harness | Provider | Interface | Source | Notes |
| --- | --- | --- | --- | --- |
| Augment Code | Augment | VS Code, JetBrains, CLI | [Documentation](https://docs.augmentcode.com/) | Agentic coding product with local and remote workflows. |
| Cline | Cline | VS Code, CLI, desktop | [Documentation](https://docs.cline.bot/) | Open-source, human-in-the-loop agent with rules, workflows, skills, and MCP. |
| Cursor Agent | Anysphere | IDE, CLI, cloud | [Documentation](https://docs.cursor.com/agent/overview) | Cursor IDE agent and background-agent surfaces. |
| GitHub Copilot agent mode | GitHub | VS Code, Visual Studio, JetBrains, others | [Documentation](https://docs.github.com/en/copilot/concepts/agents) | Interactive IDE agent; GitHub also offers a separate remote cloud agent. |
| Google Antigravity | Google | IDE | [Documentation](https://antigravity.google/docs/) | Agent-first development environment with workspace rules and skills. |
| JetBrains Junie | JetBrains | JetBrains IDEs | [Documentation](https://www.jetbrains.com/help/ai-assistant/junie-agent.html) | IDE-native surface of the Junie coding agent. |
| Kilo Code | Kilo | VS Code, JetBrains, CLI | [Documentation](https://kilocode.ai/docs/) | Open-source agent derived from the Cline/Roo Code family. |
| Kiro IDE | AWS | IDE | [Documentation](https://kiro.dev/docs/) | Agentic IDE centered on specs, steering, hooks, and MCP. |
| PearAI | PearAI | IDE | [Repository](https://github.com/trypear/pearai-app) | Open-source AI code editor; verify activity and agent capabilities before support. |
| Roo Code | Roo Code | VS Code | [Documentation](https://docs.roocode.com/) | Open-source agent with modes, rules, skills, and MCP. |
| Tabnine Agent | Tabnine | IDE | [Documentation](https://docs.tabnine.com/main/getting-started/tabnine-agent) | Commercial IDE agent with enterprise controls. |
| Trae | ByteDance | IDE | [Website](https://www.trae.ai/) | Agentic code editor with interactive and autonomous modes. |
| Void | Void | IDE | [Repository](https://github.com/voideditor/void) | Open-source Cursor alternative; verify project status before adapter work. |
| Windsurf Cascade | Cognition | IDE | [Documentation](https://docs.windsurf.com/windsurf/cascade/cascade) | Agent in the Windsurf editor with rules, workflows, skills, and MCP. |
| Zed Agent Panel | Zed Industries | Editor | [Documentation](https://zed.dev/docs/ai/agent-panel) | Agent integrated into the open-source Zed editor. |

## Remote and asynchronous coding agents

These services execute work in a hosted or isolated environment and commonly
produce a branch or pull request. Some share a brand with a local harness but
need a separate adapter because their configuration entry point differs.

| Harness | Provider | Interface | Source | Notes |
| --- | --- | --- | --- | --- |
| Claude Code on the web | Anthropic | Web, GitHub | [Documentation](https://docs.anthropic.com/en/docs/claude-code/claude-code-on-the-web) | Runs Claude Code tasks in Anthropic-managed environments. |
| Codegen | Codegen | Web, Slack, GitHub | [Documentation](https://docs.codegen.com/) | Delegated software-engineering agents connected to repositories. |
| Codex cloud | OpenAI | Web, IDE, GitHub | [Documentation](https://developers.openai.com/codex/cloud/) | Executes tasks asynchronously in isolated cloud environments. |
| Cosine Genie | Cosine | Web, Slack | [Website](https://cosine.sh/) | Commercial asynchronous software-engineering agent. |
| Cursor background agents | Anysphere | Web, IDE, Slack | [Documentation](https://docs.cursor.com/background-agents) | Remote Cursor agents that edit and run code in isolated machines. |
| Devin | Cognition | Web, Slack, IDE | [Documentation](https://docs.devin.ai/) | Hosted autonomous software-engineering agent. |
| GitHub Copilot cloud agent | GitHub | GitHub, VS Code | [Documentation](https://docs.github.com/en/copilot/concepts/agents/cloud-agent/about-cloud-agent) | GitHub-hosted agent that works independently on branches and pull requests. |
| Google Jules | Google | Web, GitHub | [Documentation](https://jules.google/docs/) | Asynchronous coding agent operating on GitHub repositories. |
| OpenHands Cloud | OpenHands | Web, GitHub, API | [Documentation](https://docs.openhands.dev/) | Hosted form of the OpenHands software-engineering agent. |
| Qodo Merge | Qodo | GitHub, GitLab, Bitbucket | [Documentation](https://docs.qodo.ai/qodo-documentation/qodo-merge) | Pull-request and repository agent, formerly PR-Agent. |
| Replit Agent | Replit | Web IDE, mobile | [Documentation](https://docs.replit.com/replitai/agent) | Hosted agent for building and modifying applications in Replit. |
| Sweep | Sweep | GitHub | [Repository](https://github.com/sweepai/sweep) | GitHub issue-to-pull-request agent; verify current product status before support. |

## Software-engineering agent runners and research harnesses

These are runnable coding agents rather than ordinary end-user editors. They are
useful adapter candidates for evaluation, CI, or self-hosted execution.

| Harness | Maintainer | Source | Notes |
| --- | --- | --- | --- |
| Agentless | OpenAutoCoder | [Repository](https://github.com/OpenAutoCoder/Agentless) | SWE-bench-oriented localization, repair, and patch-validation pipeline. |
| AutoCodeRover | AutoCodeRoverSG | [Repository](https://github.com/AutoCodeRoverSG/auto-code-rover) | Autonomous program-repair agent for repository issues. |
| Devika | StitionAI | [Repository](https://github.com/stitionai/devika) | Open-source autonomous software-engineering agent; verify maintenance before support. |
| GPT Pilot | Pythagora | [Repository](https://github.com/Pythagora-io/gpt-pilot) | Multi-step application-development agent; also distributed through Pythagora. |
| OpenHands | OpenHands | [Repository](https://github.com/OpenHands/OpenHands) | Open-source software-agent platform with local, CLI, and hosted interfaces. |
| SWE-agent | Princeton NLP | [Documentation](https://swe-agent.com/) | Open-source runner and agent-computer interface for software-engineering tasks. |

## Related tools that are not harnesses

RosettAI should not treat the following categories as target harnesses unless a
concrete coding-agent application is built on top of them:

- Agent SDKs and orchestration frameworks, including the OpenAI Agents SDK,
  Anthropic Agent SDK, Google ADK, LangGraph, AutoGen, CrewAI, Semantic Kernel,
  and PydanticAI.
- Protocols and resource formats, including MCP, Agent2Agent (A2A), Agent Client
  Protocol (ACP), and Agent Skills.
- Model APIs, autocomplete-only extensions, prompt libraries, benchmark
  leaderboards, and configuration-sync utilities.
- No-code application generators that do not operate as general coding agents
  over an existing local or remote repository.

## Maintenance rules

When updating this inventory:

1. Add only a harness with an official documentation page or canonical source
   repository.
2. Prefer the current product name and record former names in the notes.
3. List separate surfaces only when they have materially different execution or
   configuration boundaries.
4. Move discontinued projects to a clearly marked archived section rather than
   silently deleting historical adapter candidates.
5. Recheck commercial product names and URLs on every review; this market changes
   quickly.
