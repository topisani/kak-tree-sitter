# Tweaking

Here is a list of tweaks you might be interested in depending on the languages you use.

- [JSX/TSX](#jsxtsx)

## JSX/TSX

JSX and TSX grammars and queries are installed like any other languages:

```shell
ktsctl sync jsx
ktsctl sync tsx
```

However, depending on how Kakoune is performing `filetype` detection, you might not get a working setup
right off the gates. You need to ensure that `filetype` is set to either `jsx` or `tsx`, and not `javascript`
nor `typescript`.
