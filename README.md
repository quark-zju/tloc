# tloc

Tree-style Lines Of Code counter.

Like [tokei](https://github.com/XAMPPRocky/tokei) or [cloc](https://github.com/aldanial/cloc), but with directory trees.

Install: `cargo install tloc`

See [CHANGELOG.md](CHANGELOG.md) for release notes.

## Example

```text
% tloc open-webui
Files    LOC | Name                     Code Comment Blank Language
 4867 438915 | open-webui             394092   22125 22698 JSON,Python,JavaScript,Svelte,TypeScript,CSS,...
  716 229896  \ src                   221716    1572  6608 JSON,Svelte,TypeScript,CSS,JavaScript,HTML
  657 227004   | lib                  219203    1502  6299 JSON,Svelte,TypeScript,JavaScript
   63 140279    \ i18n                140264       3    12 JSON,TypeScript
   62 140191     | locales            140191       0     0 JSON
  530  59319    \ components           54494    1017  3808 Svelte,TypeScript,JavaScript
  260 176188  \ backend               145052   16442 14694 Python,JavaScript,CSS,JSON,Plain Text,INI,...
  254 175827   | open_webui           144947   16249 14631 Python,JavaScript,CSS,JSON,INI,SVG,...
    6  81990   | static                70455   11487    48 JavaScript,CSS,SVG
    2  81672   | swagger-ui            70209   11463     0 JavaScript,CSS
    1  72362   | swagger-ui-bundle.js  60899   11463     0 JavaScript
```

Less details:

```
% tloc open-webui -p 36
Files    LOC | Name                 Code Comment Blank Language
 4867 438915 | open-webui         394092   22125 22698 JSON,Python,JavaScript,Svelte,TypeScript,CSS,...
  716 229896  \ src               221716    1572  6608 JSON,Svelte,TypeScript,CSS,JavaScript,HTML
  657 227004   | lib              219203    1502  6299 JSON,Svelte,TypeScript,JavaScript
  260 176188  \ backend           145052   16442 14694 Python,JavaScript,CSS,JSON,Plain Text,INI,...
  254 175827   | open_webui       144947   16249 14631 Python,JavaScript,CSS,JSON,INI,SVG,...
```

Filter by languages:

```text
% tloc open-webui -L python,json   
Files    LOC | Name                 Code Comment Blank Language
  318 262758 | open-webui         243476    4707 14575 JSON,Python
   64 147150  \ src               147150       0     0 JSON
   64 147150   | lib              147150       0     0 JSON
   62 140191   | i18n             140191       0     0 JSON
   62 140191   | locales          140191       0     0 JSON
  246  93722  \ backend            74461    4702 14559 Python,JSON
  246  93722   | open_webui        74461    4702 14559 Python,JSON
```

Skip directories:

```text
% tloc LibreChat -X '**/node_modules'
Files    LOC | Name                 Code Comment Blank Language
 2704 573520 | LibreChat          479237   35470 58813 TypeScript,JavaScript,TSX,JSON,CSS,YAML,...
  973 232713  \ packages          185925   17329 29459 TypeScript,TSX,JSON,Markdown,JavaScript,Shell,...
  449 138526   | api              108936   10798 18792 TypeScript,Shell,JSON,JavaScript
  440 138206   | src              108650   10772 18784 TypeScript,Shell
 1178 174339  \ client            155138    4880 14321 TSX,TypeScript,JSON,CSS,JavaScript,SVG,...
 1139 173134   | src              154072    4782 14280 TSX,TypeScript,JSON,CSS,JSX,Markdown,...
  701  86761   | components        77235    2034  7492 TSX,TypeScript,JavaScript
  445 110893  \ api                85761   11377 13755 JavaScript,JSON,Handlebars
  336  84148   | server            65503    8120 10525 JavaScript,JSON,Handlebars
```