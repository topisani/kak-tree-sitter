; kak-tree-sitter notes: taken from helix-editor/helix

((comment) @injection.content
 (#set! injection.language "comment"))

((script_element
  (raw_text) @injection.content)
 (#set! injection.language "javascript"))

((style_element
  (raw_text) @injection.content)
 (#set! injection.language "css"))

((frontmatter
	(raw_text) @injection.content)
 (#set! injection.language "typescript"))

((interpolation
	(raw_text) @injection.content)
 (#set! injection.language "tsx"))
