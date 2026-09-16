# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this table.

Edit the right-hand column to match whatever vocabulary you actually use.

## Wayfinder labels

Used by `/wayfinder`: the **map** issue plus the per-type child tickets. These labels are expected to exist in the tracker alongside the triage labels above — the full set is `wayfinder:map` and the four `wayfinder:<type>` labels.

| Label                    | Meaning                                                   |
| ------------------------ | --------------------------------------------------------- |
| `wayfinder:map`          | The single map issue holding Notes / Decisions-so-far / Fog |
| `wayfinder:research`     | Child ticket: investigate / gather evidence               |
| `wayfinder:prototype`    | Child ticket: build a throwaway prototype to de-risk      |
| `wayfinder:grilling`     | Child ticket: stress-test an idea or plan                 |
| `wayfinder:task`         | Child ticket: concrete implementation task               |

Do not invent or force any additional labels; tagging strictly follows the skill rules that reference these strings.
