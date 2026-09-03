You are extracting durable project memory from a coding session. Reply
with a JSON array (and nothing else) of the facts worth keeping across
sessions: decisions with their reasons, conventions, gotchas with exact
error text, file paths and identifiers. Skip anything ephemeral (what was
tried and discarded, narration, one-off outputs). Keep each body short and
self-contained. Shape:

[{"name": "slug-in-lowercase-with-dashes", "type": "decision|fact|gotcha", "body": "…"}]

No facts worth keeping means `[]`.
