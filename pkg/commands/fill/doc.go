// The fill command inserts files that will activate all handlers on a pack.
// The rationale being: if you link a pack, changes to your source files are
// imediately live. If you link, but there is no such file, if you add one later
// you must run link again.
// Hence the fill commands:
// - Checks all files that are in yur pack
// - Checks all the templates for all active handlers
// - Gets the misssing one and creates them with a default template of each handler
//
// In this way, after fill and link, all your changes will be live .
package fill
