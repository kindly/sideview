<sv-page label="Extensions demo" category="examples" order="22">

<sv-prose id="intro">
# The extension mechanism, live

Two installed extensions (see `.sideview.toml` and `extensions/`), each a
directory with a manifest, exercising the whole of EXTENSIONS.md: the block
tag, the injected `SIDEVIEW_BLOCK` context, `call_cli` and
`call_cli_streaming`, abort-kills-child, the caps, and live theme mirroring
— flip the ◐ and both frames follow.
</sv-prose>

<sv-prose id="duckdb-intro">
## duckdb — streaming exec into a grid

The block's body is SQL on stdin; rows arrive as `-jsonlines` and paint as
they stream. Re-run aborts the previous child mid-flight — cancellation is
the fetch's AbortController, nothing more.
</sv-prose>

<sv-duckdb id="grid" height="24rem">
select
  i as n,
  i * i as square,
  printf('%.4f', sqrt(i)) as root,
  case when i % 15 = 0 then 'fizzbuzz'
       when i % 3 = 0 then 'fizz'
       when i % 5 = 0 then 'buzz'
       else '' end as game
from range(2000) t(i)
</sv-duckdb>

<sv-prose id="git-intro">
## git — the generality check

A second extension, zero shared code with the first: the body becomes argv,
`call_cli` runs it, the frame colorizes. The last commits of this very
repository:
</sv-prose>

<sv-git id="log" height="18rem">
log --oneline -15
</sv-git>

<sv-prose id="git-diff-intro">
And a real diff — what the previous commit changed:
</sv-prose>

<sv-git id="lastdiff" height="26rem">
show HEAD --stat --patch --no-color
</sv-git>

</sv-page>
