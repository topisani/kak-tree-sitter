; kak-tree-sitter notes: taken from helix/helix-editor

; Tokens
;-------

[
  ";"
  (optional_chain) ; ?.
  "."
  ","
] @punctuation.delimiter

[
  "-"
  "--"
  "-="
  "+"
  "++"
  "+="
  "*"
  "*="
  "**"
  "**="
  "/"
  "/="
  "%"
  "%="
  "<"
  "<="
  "<<"
  "<<="
  "="
  "=="
  "==="
  "!"
  "!="
  "!=="
  "=>"
  ">"
  ">="
  ">>"
  ">>="
  ">>>"
  ">>>="
  "~"
  "^"
  "&"
  "|"
  "^="
  "&="
  "|="
  "&&"
  "||"
  "??"
  "&&="
  "||="
  "??="
  "..."
] @operator

(ternary_expression ["?" ":"] @operator)

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
]  @punctuation.bracket

(template_substitution
  "${" @punctuation.special
  "}" @punctuation.special) @embedded

[
  "async"
  "debugger"
  "delete"
  "extends"
  "from"
  "get"
  "new"
  "set"
  "target"
  "typeof"
  "instanceof"
  "void"
  "with"
] @keyword

[
  "of"
  "as"
  "in"
] @keyword.operator

[
  "function"
] @keyword.function

[
  "class"
  "let"
  "var"
] @keyword.storage.type

[
  "const"
  "static"
] @keyword.storage.modifier

[
  "default"
  "yield"
  "finally"
  "do"
  "await"
] @keyword.control

[
  "if"
  "else"
  "switch"
  "case"
  "while"
] @keyword.control.conditional

[
  "for"
] @keyword.control.repeat

[
  "import"
  "export"
] @keyword.control.import 

[
  "return"
  "break"
  "continue"
] @keyword.control.return

[
  "throw"
  "try"
  "catch"
] @keyword.control.exception

; Variables
;----------

(identifier) @variable

; Properties
;-----------

(property_identifier) @variable.other.member
(private_property_identifier) @variable.other.member.private
(shorthand_property_identifier) @variable.other.member
(shorthand_property_identifier_pattern) @variable.other.member

; Function and method definitions
;--------------------------------

(function
  name: (identifier) @function)
(function_declaration
  name: (identifier) @function)
(method_definition
  name: (property_identifier) @function.method)
(method_definition
  name: (private_property_identifier) @function.method.private)

(pair
  key: (property_identifier) @function.method
  value: [(function) (arrow_function)])
(pair
  key: (private_property_identifier) @function.method.private
  value: [(function) (arrow_function)])

(assignment_expression
  left: (member_expression
    property: (property_identifier) @function.method)
  right: [(function) (arrow_function)])
(assignment_expression
  left: (member_expression
    property: (private_property_identifier) @function.method.private)
  right: [(function) (arrow_function)])

(variable_declarator
  name: (identifier) @function
  value: [(function) (arrow_function)])

(assignment_expression
  left: (identifier) @function
  right: [(function) (arrow_function)])

; Function and method parameters
;-------------------------------

; Arrow function parameters in the form `p => ...` are supported by both
; javascript and typescript grammars without conflicts.
(arrow_function
  parameter: (identifier) @variable.parameter)
  
; Function and method calls
;--------------------------

(call_expression
  function: (identifier) @function)

(call_expression
  function: (member_expression
    property: (property_identifier) @function.method))
(call_expression
  function: (member_expression
    property: (private_property_identifier) @function.method.private))

; Literals
;---------

(this) @variable.builtin
(super) @variable.builtin

[
  (true)
  (false)
  (null)
  (undefined)
] @constant.builtin

(comment) @comment

[
  (string)
  (template_string)
] @string

(regex) @string.regexp
(number) @constant.numeric.integer

; Special identifiers
;--------------------

((identifier) @constructor
 (#match? @constructor "^[A-Z]"))

([
    (identifier)
    (shorthand_property_identifier)
    (shorthand_property_identifier_pattern)
 ] @constant
 (#match? @constant "^[A-Z_][A-Z\\d_]+$"))

((identifier) @variable.builtin
 (#match? @variable.builtin "^(arguments|module|console|window|document)$")
 (#is-not? local))

(call_expression
 (identifier) @function.builtin
 (#any-of? @function.builtin
  "eval"
  "fetch"
  "isFinite"
  "isNaN"
  "parseFloat"
  "parseInt"
  "decodeURI"
  "decodeURIComponent"
  "encodeURI"
  "encodeURIComponent"
  "require"
  "alert"
  "prompt"
  "btoa"
  "atob"
  "confirm"
  "structuredClone"
  "setTimeout"
  "clearTimeout"
  "setInterval"
  "clearInterval"
  "queueMicrotask")
 (#is-not? local))

; Function and method parameters
;-------------------------------

; Javascript and Typescript Treesitter grammars deviate when defining the
; tree structure for parameters, so we need to address them in each specific
; language instead of ecma.

; (p)
(formal_parameters 
  (identifier) @variable.parameter)

; (...p)
(formal_parameters
  (rest_pattern
    (identifier) @variable.parameter))

; ({ p })
(formal_parameters
  (object_pattern
    (shorthand_property_identifier_pattern) @variable.parameter))

; ({ a: p })
(formal_parameters
  (object_pattern
    (pair_pattern
      value: (identifier) @variable.parameter)))

; ([ p ])
(formal_parameters
  (array_pattern
    (identifier) @variable.parameter))

; (p = 1)
(formal_parameters
  (assignment_pattern
    left: (identifier) @variable.parameter))
