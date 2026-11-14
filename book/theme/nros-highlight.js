// nano-ros book: languages mdBook's bundled highlight.js (10.1.1) lacks.
//
// mdBook highlights every code block in book.js, before `additional-js`
// runs, and leaves a block whose language it does not know as plain text.
// This registers the missing languages and highlights those blocks again.
// TOML, YAML, Bash, C, C++, Rust, XML and JSON are in the bundle already;
// a fence must still name its language for them to apply.
(function () {
    "use strict";
    if (typeof hljs === "undefined") {
        return;
    }

    // CMake: every command call (`name(`) reads as a keyword, so the
    // nano-ros, ament and rosidl commands colour like CMake's own.
    hljs.registerLanguage("cmake", function (hljs) {
        return {
            name: "CMake",
            aliases: ["cmake.in"],
            case_insensitive: true,
            keywords: {
                literal:
                    "PUBLIC PRIVATE INTERFACE REQUIRED COMPONENTS " +
                    "STATIC SHARED OBJECT ON OFF TRUE FALSE " +
                    "NOT AND OR EQUAL STREQUAL DEFINED",
            },
            contains: [
                {
                    className: "keyword",
                    begin: /\b[A-Za-z_][A-Za-z0-9_]*(?=\s*\()/,
                },
                { className: "variable", begin: /\$(ENV)?\{/, end: /\}/ },
                hljs.HASH_COMMENT_MODE,
                hljs.QUOTE_STRING_MODE,
                hljs.NUMBER_MODE,
            ],
        };
    });

    // JSON5 (zenoh's configuration files): the JSON grammar already takes
    // `//` comments, bare keys and trailing commas well enough.
    if (hljs.registerAliases) {
        hljs.registerAliases(["json5"], { languageName: "json" });
    }

    // A file tree (```tree): the branch lines dim, directories (a name
    // ending in `/`) in the link colour, files plain, `# notes` as comments,
    // and the `(new)` / `(changed)` marks a Tutorial chapter's tree carries.
    // The colours are nros.css's `.hljs-tree-*` rules, from the theme's own
    // variables, so every theme applies.
    hljs.registerLanguage("tree", function () {
        return {
            name: "File tree",
            contains: [
                { className: "tree-branch", begin: /[│├└┬─|`]+/ },
                // What a chapter added or changed, written after the name:
                // `├── lib.rs  (new)`, `└── system.toml  (changed)`.
                { className: "tree-new", begin: /\(new\)/ },
                { className: "tree-changed", begin: /\(changed\)/ },
                { className: "tree-comment", begin: /#/, end: /$/ },
                { className: "tree-dir", begin: /[^\s│├└┬─|`#][^\s#]*\// },
                { className: "tree-file", begin: /[^\s│├└┬─|`#][^\s#]*/ },
            ],
        };
    });

    document
        .querySelectorAll("code.language-cmake, code.language-json5, code.language-tree")
        .forEach(function (block) {
            hljs.highlightBlock(block);
        });
})();
