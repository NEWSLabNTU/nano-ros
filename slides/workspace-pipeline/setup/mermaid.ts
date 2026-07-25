import { defineMermaidSetup } from '@slidev/types'

// Mermaid v11 (bundled with this Slidev) defaults to securityLevel:'strict',
// which disables htmlLabels — so `<br/>` inside flowchart node labels throws and
// the slide renders the "An error occurred on this slide" boundary. Our diagrams
// use `<br/>` for multi-line node labels, so opt back into html labels.
export default defineMermaidSetup(() => ({
  securityLevel: 'loose',
  flowchart: { htmlLabels: true },
}))
