# Archive it!
Allow you to archive a website locally.

# Installing
```shell
cargo install <git-url-of-this-repo>
```
# Usage

To archive content from docs.rs you'll need something like this
```shell
# -s : secure upstream
# -l : listen port
# <upstream host>
# <output folder>
archive-it -s -l 8000 docs.rs docs-archive
```
and then navigate to localhost:8000 (url will show up in console)  
it will archive all resource that you navigate through it.
> use `archive-it --help` for more option

### Features
+ Store different query in different state
  > `/search?q=A` and `/search?q=B` will store in different state
+ Can replace upstream url with given value 
  > this will allow to open html file directly but above feature will not work