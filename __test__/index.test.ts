import { describe, expect, test } from "bun:test";
import { Glob, GlobSet, isMatch, makeRe, scan } from "../index.js";

test("literal match", () => {
  expect(isMatch("foo.txt", "foo.txt")).toBe(true);
  expect(isMatch("foo.txt", "bar.txt")).toBe(false);
});

test("star and globstar", () => {
  expect(isMatch("foo/bar/baz.js", "**/*.js")).toBe(true);
  expect(isMatch("a/b/c/d.js", "a/**/d.js")).toBe(true);
  expect(isMatch("a/b/c/d.ts", "a/**/d.js")).toBe(false);
  expect(isMatch("a/b", "a/**/b")).toBe(true);
});

test("question mark", () => {
  expect(isMatch("abc", "a?c")).toBe(true);
  expect(isMatch("abbc", "a?c")).toBe(false);
});

test("character classes", () => {
  expect(isMatch("abc", "[abc]bc")).toBe(true);
  expect(isMatch("zbc", "[a-z]bc")).toBe(true);
  expect(isMatch("Abc", "[a-z]bc")).toBe(false);
  expect(isMatch("Abc", "[a-z]bc", { nocase: true })).toBe(true);
});

test("braces", () => {
  expect(isMatch("foo.js", "*.{js,ts}")).toBe(true);
  expect(isMatch("foo.ts", "*.{js,ts}")).toBe(true);
  expect(isMatch("foo.css", "*.{js,ts}")).toBe(false);
  expect(isMatch("a/x.js", "a/{x,y}.js")).toBe(true);
});

test("dotfiles", () => {
  expect(isMatch(".env", "*")).toBe(false);
  expect(isMatch(".env", "*", { dot: true })).toBe(true);
  expect(isMatch(".env", ".*")).toBe(true);
});

test("negation", () => {
  expect(isMatch("foo.txt", "!*.txt")).toBe(false);
  expect(isMatch("foo.md", "!*.txt")).toBe(true);
});

test("extglob", () => {
  expect(isMatch("foo.js", "*.@(js|ts)")).toBe(true);
  expect(isMatch("foo.bar.js", "foo.+(bar|baz).js")).toBe(true);
  expect(isMatch("foo.js", "!(*.ts)")).toBe(true);
});

test("compiled Glob reuse", () => {
  const g = new Glob("**/*.{js,ts}");
  expect(g.test("src/a.js")).toBe(true);
  expect(g.test("src/lib/a.ts")).toBe(true);
  expect(g.test("src/a.css")).toBe(false);
});

test("GlobSet", () => {
  const s = new GlobSet(["**/*.js", "**/*.ts", "!**/node_modules/**"]);
  expect(s.test("src/a.js")).toBe(true);
  expect(s.test("src/a.ts")).toBe(true);
});

test("scan extracts prefix and glob", () => {
  const r = scan("src/**/*.js");
  expect(r.base).toBe("src");
  expect(r.glob).toBe("**/*.js");
  expect(r.isGlob).toBe(true);
  expect(r.isGlobstar).toBe(true);
});

test("makeRe emits a string", () => {
  const re = makeRe("*.js");
  expect(typeof re).toBe("string");
});

test("windows separators", () => {
  expect(isMatch("a\\b\\c.js", "**/*.js", { windows: true })).toBe(true);
  expect(isMatch("a/b/c.js", "**/*.js", { windows: true })).toBe(true);
});

test("case insensitive", () => {
  expect(isMatch("FOO.JS", "*.js", { nocase: true })).toBe(true);
});

test("matchBase", () => {
  expect(isMatch("a/b/c.js", "*.js", { matchBase: true })).toBe(true);
  expect(isMatch("a/b/c.js", "*.js")).toBe(false);
});

test("bare star matches whole component", () => {
  expect(isMatch("abc", "*")).toBe(true);
  expect(isMatch("", "*")).toBe(true);
  expect(isMatch("a/b", "*")).toBe(false);
  expect(isMatch("a/b", "**")).toBe(true);
  expect(isMatch("", "**")).toBe(true);
});
