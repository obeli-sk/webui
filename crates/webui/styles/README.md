# WebUI Styles

This directory contains the SCSS stylesheets for the Obelisk WebUI, organized using the 7-1 pattern (simplified).

## Directory Structure

```
styles/
├── styles.scss         # Main entry point - imports all partials
├── base/               # Foundation styles
│   ├── _variables.scss # Design tokens (colors, spacing, fonts)
│   └── _reset.scss     # CSS reset and body defaults
├── layout/             # Page structure
│   ├── _container.scss # Main container and pagination
│   └── _navigation.scss # Navigation bar
├── components/         # Reusable UI components
│   ├── _header.scss    # Header and execution header
│   ├── _badges.scss    # Badges and labels
│   ├── _tables.scss    # Table styles
│   ├── _buttons.scss   # Button variants
│   ├── _forms.scss     # Form elements and inputs
│   ├── _filters.scss   # Filter UI components
│   ├── _code-block.scss # Code blocks and syntax highlighting
│   ├── _tree.scss      # Custom tree component
│   └── _actions.scss   # Action buttons (replay, upgrade)
└── pages/              # Page-specific styles
    ├── _trace.scss     # Trace view page
    ├── _timeline.scss  # Timeline/execution log detail
    ├── _logs.scss      # Logs page
    └── _definitions.scss # World/type definitions page
```

## Naming Conventions

- **Partials**: Files prefixed with `_` are partials (not compiled directly)
- **BEM-like**: Classes follow a component-based naming pattern
- **Variables**: Prefixed by category (`$color-`, `$spacing-`, `$font-`)

## Design Tokens

All design tokens are defined in `base/_variables.scss`:

- **Colors**: Background, text, accent, status, and execution state colors
- **Typography**: Font families, sizes, and line heights
- **Spacing**: Consistent spacing scale (xs, sm, md, lg, xl)
- **Borders**: Border radius variants
- **Shadows**: Shadow definitions
- **Transitions**: Animation timing

## Usage

The `styles.scss` file imports all partials using the modern `@use` syntax.
Trunk automatically compiles this to CSS during the build process.

## Adding New Styles

1. Create a new partial in the appropriate directory
2. Add `@use '../base/variables' as *;` at the top to access design tokens
3. Import the partial in `main.scss`
