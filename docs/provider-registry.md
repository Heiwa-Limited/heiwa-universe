# Provider Registry

## Runtime and infrastructure providers

| Role                          | Provider                                       | Status                          | Notes                                                                                        |
| :---------------------------- | :--------------------------------------------- | :------------------------------ | :------------------------------------------------------------------------------------------- |
| Installed operator runtime    | `heiwa` on the user's machine                  | Active                          | Primary cockpit, routing, bounded execution, and provider-wrapping surface                   |
| Local/provider runtimes       | Ollama, provider CLIs, API providers           | Wrapped                         | Provider-owned auth, quota, inference, and native tooling remain provider-owned              |
| Evidence and recall           | Local JSONL + Lance                            | Active                          | JSONL is canonical evidence; Lance is a derived, rebuildable local recall index              |
| Source control / CI / release | GitHub                                         | Active                          | Repo, pull requests, Actions, Releases, and release evidence                                 |
| Public docs                   | GitHub Pages                                   | Active                          | MkDocs Material documentation from this repository                                           |
| Hosted services               | Owner-managed local or Cloudflare-backed hosts | Optional support infrastructure | Disabled for the owner-first phase unless a specific service graduates with approval         |
| Public edge                   | Cloudflare                                     | Support infrastructure          | DNS, edge, and proxy surface where needed                                                    |
| Internal vertical runtimes    | Isolated service providers                     | Internal preview                | Surfaces such as trading stay isolated until they graduate into first-class product surfaces |

## Public-safe posture

- Public shells should defer privileged decisions to the installed runtime or hosted runtime API.
- `app.heiwa.ltd` stays the canonical user home for the visual shell; it is not a second control plane.
- Hosted services must remain scoped support surfaces, not the default operator story.
- New providers should not be added to the public story until they are verified and necessary.
