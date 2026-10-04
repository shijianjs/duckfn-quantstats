import {fileURLToPath} from 'node:url';

import {declareDocsTests} from 'duckfn-docs-kit/sql/playwright';

declareDocsTests({siteDir: fileURLToPath(new URL('..', import.meta.url))});
