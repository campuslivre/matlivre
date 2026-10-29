# Input: classified.tsv (folder<TAB>name<TAB>category<TAB>reason). Output: docs/matlab-coverage.md.
BEGIN { FS = "\t" }
{
  f = $1; n = $2; c = $3; r = $4; folders[f] = 1; cnt[f, c]++; tot[c]++
  if (c == "todo") todo[f] = todo[f] (todo[f] == "" ? "" : "\n") "- [ ] `" n "`"
  else if (c == "done") done[f] = done[f] (done[f] == "" ? "" : ", ") "`" n "`"
  else if (c == "branch") br[f] = br[f] (br[f] == "" ? "" : ", ") "`" n "`"
  else if (c == "keyword") kwl[f] = kwl[f] (kwl[f] == "" ? "" : ", ") "`" n "`"
  else if (c == "out") { outr[f, r] = outr[f, r] (outr[f, r] == "" ? "" : ", ") "`" n "`"; reasons[f, r] = 1 }
}
END {
  print "# MATLAB coverage"
  print ""
  print "Every public function of MATLAB R2023b in scope for matlivre, from the `Contents.m` files and the"
  print "public function files and classes of these folders of the installation, checked against matlivre."
  print "Scope: the core MATLAB language, math, data analysis, strings, data types, dates, file I/O and"
  print "graphics (`toolbox/matlab/...`), and the Symbolic Math Toolbox (`toolbox/symbolic`)."
  print ""
  print "Regenerate with `tools/coverage/update.sh` after merging work (" date ")."
  print ""
  print "| Status | Count |"
  print "|---|---:|"
  print "| Implemented | " tot["done"] + 0 " |"
  print "| In a branch not merged yet | " tot["branch"] + 0 " |"
  print "| Language keyword (check it works) | " tot["keyword"] + 0 " |"
  print "| **To implement** | **" tot["todo"] + 0 "** |"
  print "| Out of scope | " tot["out"] + 0 " |"
  print ""
  print "Out of scope, with the reason given for each: Java/.NET/COM/C interop and MEX, desktop/IDE/GUI and"
  print "printing tools, audio/video devices, scientific file formats (HDF5, netCDF, CDF, FITS, TIFF objects)"
  print "and web services, map tiles, internal helpers, and MATLAB example files."
  print ""
  n = asorti(folders, fs)
  for (i = 1; i <= n; i++) {
    f = fs[i]
    print "## " f
    print ""
    print "Implemented: " (cnt[f, "done"] + 0) ". To implement: " (cnt[f, "todo"] + 0) ". In a branch: " (cnt[f, "branch"] + 0) ". Out of scope: " (cnt[f, "out"] + 0) "."
    print ""
    if (todo[f] != "") { print "To implement:"; print ""; print todo[f]; print "" }
    if (br[f] != "") { print "In a branch: " br[f]; print "" }
    if (kwl[f] != "") { print "Language keywords: " kwl[f]; print "" }
    if (done[f] != "") { print "Implemented: " done[f]; print "" }
    m = asorti(reasons, rs)
    for (k = 1; k <= m; k++) { split(rs[k], p, SUBSEP); if (p[1] == f) print "Out of scope (" p[2] "): " outr[f, p[2]] "\n" }
  }
}
