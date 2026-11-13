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

    document
        .querySelectorAll("code.language-cmake, code.language-json5")
        .forEach(function (block) {
            hljs.highlightBlock(block);
        });
})();
