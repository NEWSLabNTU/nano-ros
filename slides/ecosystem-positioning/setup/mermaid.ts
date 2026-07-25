import { defineMermaidSetup } from '@slidev/types'

// Mermaid v11 defaults to securityLevel:'strict' (htmlLabels off), which makes
// `<br/>` in flowchart labels throw the per-slide error boundary. Opt back in so
// multi-line node labels render.
export default defineMermaidSetup(() => ({
  securityLevel: 'loose',
  flowchart: { htmlLabels: true },
}))
