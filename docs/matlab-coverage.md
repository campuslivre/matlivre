# MATLAB coverage

Every public function of MATLAB R2023b in scope for matlivre, from the `Contents.m` files and the
public function files and classes of these folders of the installation, checked against matlivre.
Scope: the core MATLAB language, math, data analysis, strings, data types, dates, file I/O and
graphics (`toolbox/matlab/...`), and the Symbolic Math Toolbox (`toolbox/symbolic`).

Regenerate with `tools/coverage/update.sh` after merging work (2026-10-29).

| Status | Count |
|---|---:|
| Implemented | 0 |
| In a branch not merged yet | 0 |
| Language keyword (check it works) | 0 |
| **To implement** | **0** |
| Out of scope | 0 |

Out of scope, with the reason given for each: Java/.NET/COM/C interop and MEX, desktop/IDE/GUI and
printing tools, audio/video devices, scientific file formats (HDF5, netCDF, CDF, FITS, TIFF objects)
and web services, map tiles, internal helpers, and MATLAB example files.

