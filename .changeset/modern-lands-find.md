---
"@biomejs/biome": patch
---

Added a new nursery rule [`noBannedDependencies`](https://biomejs.dev/linter/rules/no-banned-dependencies/), which flags imports and `package.json` dependency entries that have better alternatives in e18e's module replacement data.

For example, the following are reported:

```js
import runAll from "npm-run-all"
```

```json
{
  "dependencies": {
    "npm-run-all": "^4.1.5"
  }
}
```
