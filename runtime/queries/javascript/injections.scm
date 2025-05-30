; kak-tree-sitter notes: taken from helix/helix-editor

; Parse the contents of tagged template literals using
; a language inferred from the tag.

(call_expression
  function: [
    (identifier) @injection.language
    (member_expression
      property: (property_identifier) @injection.language)
  ]
  arguments: (template_string) @injection.content
  (#any-of? @injection.language "html" "css" "json" "sql" "js" "ts" "bash"))

; Parse the contents of $ template literals as shell commands

(call_expression
  function: [
    (identifier) @_template_function_name
    (member_expression
      property: (property_identifier) @_template_function_name)
  ]
  arguments: (template_string) @injection.content
 (#eq? @_template_function_name "$")
 (#set! injection.language "bash"))

; Parse the contents of gql template literals

((call_expression
   function: (identifier) @_template_function_name
   arguments: (template_string) @injection.content)
 (#eq? @_template_function_name "gql")
 (#set! injection.language "graphql"))

; Parse regex syntax within regex literals

((regex_pattern) @injection.content
 (#set! injection.language "regex"))

; Parse JSDoc annotations in multiline comments

((comment) @injection.content
 (#set! injection.language "jsdoc")
 (#match? @injection.content "^/\\*+"))

; Parse general tags in single line comments

((comment) @injection.content
 (#set! injection.language "comment")
 (#match? @injection.content "^//"))
