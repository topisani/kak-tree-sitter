; kak-tree-sitter notes: taken from helix/helix-editor

[
  (array)
  (object)
  (arguments)
  (formal_parameters)

  (statement_block)
  (switch_statement)
  (object_pattern)
  (class_body)
  (named_imports)

  (binary_expression)
  (return_statement)
  (template_substitution)
  (export_clause)
] @indent

[
  (switch_case)
  (switch_default)
] @indent @extend

[
  "}"
  "]"
  ")"
] @outdent

[
  (jsx_element)
  (jsx_self_closing_element)
] @indent

(parenthesized_expression) @indent
