# Tag lists

The four CSV files here are copied unchanged from krita-ai-diffusion 1.55.5
(<https://github.com/Acly/krita-ai-diffusion>, `ai_diffusion/tags/`):

- `Danbooru.csv` and `Danbooru NSFW.csv`: tags from Danbooru's public tag dataset
  (danbooru.donmai.us). A tag counts as SFW when it is only used on `rating:general` posts.
- `e621.csv` and `e621 NSFW.csv`: tags from e621.net.

Columns: `tag,type,count,aliases`. `type` is Danbooru's category (0 general, 1 artist, 3 copyright,
4 character, 5 meta); `count` is how often the tag is used on its site. The lists were cut to tags
used at least 20 times (general, meta) or 50 times (the others).

They are used for prompt autocompletion only (`src/tags.rs`), and the NSFW lists only when the
person turns NSFW tags on.
