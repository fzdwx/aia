use super::{DiffTheme, compute_patch_diff};

#[test]
fn patch_highlighting_preserves_tsx_context_for_inserted_attribute_line() {
    let patch = "diff --git a/RetrySummaryList.tsx b/RetrySummaryList.tsx
--- a/RetrySummaryList.tsx
+++ b/RetrySummaryList.tsx
@@ -1,5 +1,6 @@
 function RetrySummaryList() {
   return (
     <div
+      className=\"space-y-2\"
     >
       hello
 ";

    let response = compute_patch_diff(patch, DiffTheme::Dark);
    let inserted = response
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .find(|line| line.kind == "add")
        .expect("expected inserted line");

    assert!(inserted.html.contains("className"));
    assert!(inserted.html.contains("color:#79b8ff") || inserted.html.contains("color:#9ecbff"));
}
