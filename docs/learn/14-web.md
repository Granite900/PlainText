# 14. Talking to the web

**Goal:** Fetch text or JSON, and convert PlainText values to/from JSON.

## Turn it on

```plaintext
import web
```

**Security note:** `import web` can talk to the real network. Only call URLs you trust.
For demos and CI, you can pass a **local file path** instead of `https://…` — `web.get` /
`web.get_json` will read the file.

## Get text or JSON

```plaintext
import web

page = web.get("https://example.com")
data = web.get_json("examples/fixtures/sample.json")   // offline fixture
print(data["name"])
```

`web.post_json(url, value)` sends JSON to an `http://` or `https://` address and returns the
response body as text.

## Convert values

```plaintext
text = to_json(dictionary { "n": 1, "ok": true })
value = parse_json("{\"x\": 2}")
```

Numbers, text, booleans, `nothing`, lists, and dictionaries round-trip. Other types
(networks, bodies, functions, …) can't be turned into JSON.

## Practice

```bash
plaintext run examples/fetch.pt
```

Then uncomment the live URL comments in that file if you want to hit a public API.

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `web.get` needs the web module | Add `import web` |
| `post_json` with a local path | Use an `http://` or `https://` URL |
| Bad JSON text | Check quotes and commas; messages come from the parser |

**Previous:** [Neural networks ←](13-neural-networks.md)
