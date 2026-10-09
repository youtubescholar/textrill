# Upstream fixtures, tracked

`tfiles/` and `workflows/` are byte-identical copies of files from the upstream
`txt2html` 3.0 distribution, which this project tracks as the tarball
`research/txt2html-3.0.tar.gz`. They are copied, not symlinked, so the two
corpus runners — the Perl differential (`run.sh`) and the reference-free
acceptance runner (`accept.sh`) — read the same tracked files and a fresh
clone can run `make accept` without `make ref`.

Origin:

- `tfiles/` — copied from `ref/txt2html-3.0/tfiles/` (the upstream test-input and
  author-golden directory).
- `workflows/` — copied from `ref/txt2html-3.0/.github/workflows/` (the fixture
  files used by the upstream 3.0 CI smoke test, which pins the
  `--links_dictionaries` fix).

License: GPL-3.0-or-later, same as the upstream distribution. They are test
fixtures, not textrill code.

How to refresh, if the pinned tarball is ever updated:

```sh
make ref
rsync -a --delete ref/txt2html-3.0/tfiles/ textrill/tests/corpus/tfiles/
cp -p ref/txt2html-3.0/.github/workflows/test{1,2,3}.txt \
      ref/txt2html-3.0/.github/workflows/xyz.dict textrill/tests/corpus/workflows/
diff -r ref/txt2html-3.0/tfiles textrill/tests/corpus/tfiles
```

Then review: `make corpus` and `make accept` must both stay green, and
`cases.sh`'s `INPUT`/`DICT`/`append_file` paths must still resolve.