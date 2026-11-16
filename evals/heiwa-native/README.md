# Heiwa native tools

Runs the shipped local agent instructions, native tool descriptions and schemas against an installed Ollama model. Three samples per case check packaged daily briefs, honest review-only proposals, instruction-like Notes content, whether an event staged from an email names that email as its source, whether "run mail intake" uses the packaged intake, and whether a weekday in an email becomes the right date when only the agent's own instructions say what day it is. All tool responses are synthetic; this evaluation never calls an Apple app or modifies user data.

From the repository root, run `node evals/heiwa-native/run.ts --out <directory> --model gemma4:latest`. The model must declare completion and tool support. Results include each tool call and answer. There is no paid API use. Live native execution is verified separately through the app and CLI.
