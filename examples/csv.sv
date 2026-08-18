<sv-page label="CSV viewer" category="examples" order="23">

<sv-prose id="intro">
# sv-csv — the table's default tier

No SQL, no engine, no JavaScript rendering: the block references a CSV and
the daemon renders a real table — commentable (select any cell text),
snapshot-able, styled by the page. Produce the file with whatever tool you
have; overwrite it and the block re-renders on the next tick.
</sv-prose>

<sv-csv id="plain" src="examples/data/stations.csv">
</sv-csv>

<sv-prose id="diff-intro">
## A data diff, pre-computed

The agent compares; sideview colors. A `_sv_row` directive column carrying
`add` / `del` / `mod` tints rows in the diff duotone and never displays:
</sv-prose>

<sv-csv id="diffed" src="examples/data/stations-diff.csv">
</sv-csv>

<sv-prose id="freeze-intro">
## Frozen header and columns

`freeze="2"` pins the first two columns through horizontal scroll; the
header is always frozen. `height` bounds the block with its own scroll:
</sv-prose>

<sv-csv id="wide" src="examples/data/wide.csv" freeze="2" height="22rem">
</sv-csv>

</sv-page>
