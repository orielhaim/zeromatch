import test from "ava";
import { Glob, GlobSet, isMatch, makeRe, scan } from "../index.js";

test("literal match", (t) => {
  t.true(isMatch("foo.txt", "foo.txt"));
  t.false(isMatch("foo.txt", "bar.txt"));
});

test("star and globstar", (t) => {
  t.true(isMatch("foo/bar/baz.js", "**/*.js"));
  t.true(isMatch("a/b/c/d.js", "a/**/d.js"));
  t.false(isMatch("a/b/c/d.ts", "a/**/d.js"));
  t.true(isMatch("a/b", "a/**/b"));
});

test("question mark", (t) => {
  t.true(isMatch("abc", "a?c"));
  t.false(isMatch("abbc", "a?c"));
});

test("character classes", (t) => {
  t.true(isMatch("abc", "[abc]bc"));
  t.true(isMatch("zbc", "[a-z]bc"));
  t.false(isMatch("Abc", "[a-z]bc"));
  t.true(isMatch("Abc", "[a-z]bc", { nocase: true }));
});

test("braces", (t) => {
  t.true(isMatch("foo.js", "*.{js,ts}"));
  t.true(isMatch("foo.ts", "*.{js,ts}"));
  t.false(isMatch("foo.css", "*.{js,ts}"));
  t.true(isMatch("a/x.js", "a/{x,y}.js"));
});

test("dotfiles", (t) => {
  t.false(isMatch(".env", "*"));
  t.true(isMatch(".env", "*", { dot: true }));
  t.true(isMatch(".env", ".*"));
});

test("negation", (t) => {
  t.false(isMatch("foo.txt", "!*.txt"));
  t.true(isMatch("foo.md", "!*.txt"));
});

test("extglob", (t) => {
  t.true(isMatch("foo.js", "*.@(js|ts)"));
  t.true(isMatch("foo.bar.js", "foo.+(bar|baz).js"));
  t.true(isMatch("foo.js", "!(*.ts)"));
});

test("compiled Glob reuse", (t) => {
  const g = new Glob("**/*.{js,ts}");
  t.true(g.test("src/a.js"));
  t.true(g.test("src/lib/a.ts"));
  t.false(g.test("src/a.css"));
});

test("GlobSet", (t) => {
  const s = new GlobSet(["**/*.js", "**/*.ts", "!**/node_modules/**"]);
  t.true(s.test("src/a.js"));
  t.true(s.test("src/a.ts"));
});

test("scan extracts prefix and glob", (t) => {
  const r = scan("src/**/*.js");
  t.is(r.base, "src");
  t.is(r.glob, "**/*.js");
  t.true(r.isGlob);
  t.true(r.isGlobstar);
});

test("makeRe emits a string", (t) => {
  const re = makeRe("*.js");
  t.is(typeof re, "string");
});

test("windows separators", (t) => {
  t.true(isMatch("a\\b\\c.js", "**/*.js", { windows: true }));
  t.true(isMatch("a/b/c.js", "**/*.js", { windows: true }));
});

test("case insensitive", (t) => {
  t.true(isMatch("FOO.JS", "*.js", { nocase: true }));
});

test("matchBase", (t) => {
  t.true(isMatch("a/b/c.js", "*.js", { matchBase: true }));
  t.false(isMatch("a/b/c.js", "*.js"));
});

test('bare star matches whole component', t => {
  t.true(isMatch('abc', '*'))
  t.true(isMatch('', '*'))
  t.false(isMatch('a/b', '*'))
  t.true(isMatch('a/b', '**'))
  t.true(isMatch('', '**'))
})
