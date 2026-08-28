#!/usr/bin/env node
// zen-garden-resolve <name> — the connection promise as output (J1):
// one URI per line, nothing else. Scripts pipe it; humans read it.

import { resolve } from "./resolve.js";

const name = process.argv[2];
if (!name) {
  console.error("usage: zen-garden-resolve <name>");
  process.exit(1);
}

try {
  const answer = await resolve(name);
  console.log(answer.uri);
} catch (e) {
  console.error(`zen-garden-resolve: ${e.message}`);
  process.exit(1);
}
