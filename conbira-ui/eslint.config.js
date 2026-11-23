import { dirname, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import js from '@eslint/js'
import { defineConfig, globalIgnores } from 'eslint/config'
import reactHooks from 'eslint-plugin-react-hooks'
import globals from 'globals'
import tseslint from 'typescript-eslint'

const sourceRoot = fileURLToPath(new URL('./src', import.meta.url))

const featureBoundaries = {
  meta: {
    messages: {
      layer: '{{layer}} cannot import from {{target}}.',
      publicApi: 'Import the feature through {{entry}}.',
    },
  },
  create(context) {
    const [layer, feature, folder] = relative(sourceRoot, context.filename).split(sep)

    function checkImport(source) {
      if (typeof source?.value !== 'string') return
      const specifier = source.value
      const target = specifier.startsWith('@/')
        ? resolve(sourceRoot, specifier.slice(2))
        : specifier.startsWith('.')
          ? resolve(dirname(context.filename), specifier)
          : null
      if (!target) return

      const [targetLayer, targetFeature] = relative(sourceRoot, target).split(sep)
      const sameFeature = layer === 'features' && feature === targetFeature
      const canUseOtherFeatures = layer === 'app' || (layer === 'features' && folder === 'pages')
      const forbiddenLayer =
        (targetLayer === 'app' && (layer === 'shared' || layer === 'features')) ||
        (targetLayer === 'features' && !sameFeature && !canUseOtherFeatures)

      if (forbiddenLayer) {
        context.report({ node: source, messageId: 'layer', data: { layer, target: specifier } })
      } else if (targetLayer === 'features' && !sameFeature) {
        const entry = `@/features/${targetFeature}`
        if (specifier !== entry) {
          context.report({ node: source, messageId: 'publicApi', data: { entry } })
        }
      }
    }

    return {
      ImportDeclaration: (node) => checkImport(node.source),
      ExportNamedDeclaration: (node) => checkImport(node.source),
      ExportAllDeclaration: (node) => checkImport(node.source),
      ImportExpression: (node) => checkImport(node.source),
      TSImportType: (node) => checkImport(node.source),
    }
  },
}

export default defineConfig([
  globalIgnores(['dist', 'node_modules']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [js.configs.recommended, tseslint.configs.recommended, reactHooks.configs.flat.recommended],
    languageOptions: { globals: globals.browser },
  },
  {
    files: ['src/**/*.{ts,tsx}'],
    ignores: ['**/*.test.{ts,tsx}'],
    plugins: { architecture: { rules: { 'feature-boundaries': featureBoundaries } } },
    rules: { 'architecture/feature-boundaries': 'error' },
  },
])
