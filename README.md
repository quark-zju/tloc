# tloc

Tree-style Lines Of Code counter.

Like [tokei](https://github.com/XAMPPRocky/tokei) or [cloc](https://github.com/aldanial/cloc), but with directory trees.


## Example

```text
% tloc open-webui
Files    LOC | Name                           Code Comment Blank Language
 4867 438915 | open-webui                   394092   22125 22698 JSON,Python,JavaScript,Svelte,TypeScript,CSS,...
  716 229896  \ src                         221716    1572  6608 JSON,Svelte,TypeScript,CSS,JavaScript,HTML
  657 227004   | lib                        219203    1502  6299 JSON,Svelte,TypeScript,JavaScript
   63 140279    \ i18n                      140264       3    12 JSON,TypeScript
   62 140191     | locales                  140191       0     0 JSON
  530  59319    \ components                 54494    1017  3808 Svelte,TypeScript,JavaScript
  124  19161     \ chat                      17860     250  1051 Svelte
   22   5361      \ Settings                  4948      13   400 Svelte
    1   1646       \ Advanced                 1566       0    80 Svelte
    1   1646        | AdvancedParams.svelte   1566       0    80 Svelte
    1   1111       \ Interface.svelte          998       0   113 Svelte
   36   4182      \ Messages                  3925      63   194 Svelte
   13    941       \ Markdown                  876      36    29 Svelte
    1    420        | MarkdownTokens.svelte    411       2     7 Svelte
    1    852       \ ResponseMessage.svelte    807       3    42 Svelte
   56  13822     \ admin                     12689     128  1005 Svelte
   27   8728      \ Settings                  8012      26   690 Svelte
   10   2398      \ Users                     2148      72   178 Svelte
    5   1467       \ Groups                   1294      71   102 Svelte
    1    926        | Permissions.svelte       856       4    66 Svelte
    1    406       \ UserList.svelte           379       1    26 Svelte
  260 176188  \ backend                     145052   16442 14694 Python,JavaScript,CSS,JSON,Plain Text,INI,...
  254 175827   | open_webui                 144947   16249 14631 Python,JavaScript,CSS,JSON,INI,SVG,...
    6  81990   | static                      70455   11487    48 JavaScript,CSS,SVG
    2  81672   | swagger-ui                  70209   11463     0 JavaScript,CSS
    1  72362   | swagger-ui-bundle.js        60899   11463     0 JavaScript
```

Filter by language:

```text
% tloc open-webui -L python
Files   LOC | Name                Code Comment Blank Language
  240 90812 | open-webui         71530    4707 14575 Python
  238 90719 | backend            71458    4702 14559 Python
  238 90719 | open_webui         71458    4702 14559 Python
   30 25676  \ routers           19980    1503  4193 Python
   43 17817  \ utils             13990    1053  2774 Python
    1  5120   | middleware.py     4094     309   717 Python
```
