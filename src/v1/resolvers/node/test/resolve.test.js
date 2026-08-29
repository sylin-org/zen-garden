// Resolver unit tests: the pure laws (matcher, URI form). Run: npm test
import assert from "node:assert/strict";
import { test } from "node:test";
import { serviceMatches, connectionUri, parseSelectors, satisfiesWish } from "../src/resolve.js";

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

// --- capability wishes (W1) ---

test("a wish matches only when the service HOLDS the item", () => {
  const svc = { capabilities: { model: ["llama3:latest", "all-minilm:latest"] } };
  const selectors = parseSelectors("model:llama3:latest");
  assert.ok(satisfiesWish(svc, selectors));
  assert.ok(satisfiesWish(svc, parseSelectors("model:llama3")), "tag-default spelling matches");
  assert.equal(satisfiesWish(svc, parseSelectors("model:mistral")), false);
  assert.equal(satisfiesWish(svc, parseSelectors("plugin:pdf")), false);
  assert.equal(satisfiesWish({}, parseSelectors("model:llama3")), false);
});

test("a pipe continues the previous type; a comma starts a new pair", () => {
  assert.deepEqual(parseSelectors("model:llama3|mistral"), [
    { kind: "model", item: "llama3" },
    { kind: "model", item: "mistral" },
  ]);
  assert.deepEqual(parseSelectors("model:llama3, multi:10"), [
    { kind: "model", item: "llama3" },
    { kind: "multi", item: "10" },
  ]);
});

test("selectorless fragments refuse with the teaching error", () => {
  assert.throws(() => parseSelectors("model"), /needs type:item/);
  assert.throws(() => parseSelectors(""), /needs type:item/);
});
