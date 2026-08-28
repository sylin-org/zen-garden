// Resolver unit tests: the pure laws (matcher, URI form). Run: npm test
import assert from "node:assert/strict";
import { test } from "node:test";
import { serviceMatches, connectionUri } from "../src/resolve.js";

test("a bare-stem wish accepts any instance of the capability", () => {
  assert.equal(serviceMatches("witness-db::default", false, "witness-db::garden"), true);
});

test("a named instance wants exactly itself", () => {
  assert.equal(serviceMatches("redis::prod", true, "redis::default"), false);
  assert.equal(serviceMatches("redis::prod", true, "redis::prod"), true);
});

test("strangers never match", () => {
  assert.equal(serviceMatches("redis::default", false, "mongodb::garden"), false);
});

test("the connection promise carries the port when there is one", () => {
  assert.equal(connectionUri("redis", "192.168.1.195", 6379), "redis://192.168.1.195:6379");
  assert.equal(connectionUri("redis", "192.168.1.195", null), "redis://192.168.1.195");
});
