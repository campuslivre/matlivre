# MATLAB coverage

Every function in the MATLAB R2023b and Symbolic Math Toolbox function references (the
installation's `help/*/helpfuncbycat.xml`), plus key classes and undocumented public functions
found in the toolbox folders, checked against matlivre. Regenerate with
`tools/coverage/update.sh` after merging work (2026-10-29).

| Status | Count |
|---|---:|
| Implemented | 658 |
| In a branch not merged yet | 0 |
| Language keyword (check it works) | 25 |
| **To implement** | **1364** |
| Out of scope | 1332 |

Out of scope, by category of the function reference: app building, external language
interfaces (Python, Java, .NET, C/C++, COM, web services), desktop/help/add-on/startup
settings, software-development tools (projects, source control, test frameworks, debugging,
background pools, toolbox packaging), big data (tall arrays, datastores), hardware and network
communication, web access, scientific file formats (HDF4/HDF5, netCDF, CDF, FITS), audio/video
devices, interactive visual exploration, quantum computing and MuPAD migration helpers.

## MATLAB > App Building > Create Custom UI Components

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 2.

Out of scope: `appdesigner.customcomponent.configureMetadata` (App Building), `appdesigner.customcomponent.removeMetadata` (App Building)

## MATLAB > App Building > Develop Apps Programmatically

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 86.

Out of scope: `actxcontrol` (App Building), `addStyle` (App Building), `align` (App Building), `collapse` (App Building), `dialog` (App Building), `errordlg` (App Building), `export2wsdlg` (App Building), `exportsetupdlg` (App Building), `focus` (App Building), `getappdata` (App Building), `getpixelposition` (App Building), `guidata` (App Building), `guihandles` (App Building), `helpdlg` (App Building), `inputdlg` (App Building), `isappdata` (App Building), `isInScrollView` (App Building), `listdlg` (App Building), `listfonts` (App Building), `menu` (App Building), `move` (App Building), `movegui` (App Building), `msgbox` (App Building), `printdlg` (App Building), `printpreview` (App Building), `questdlg` (App Building), `removeStyle` (App Building), `rmappdata` (App Building), `scroll` (App Building), `sendEventToHTMLSource` (App Building), `setappdata` (App Building), `setpixelposition` (App Building), `textwrap` (App Building), `uialert` (App Building), `uiaxes` (App Building), `uibutton` (App Building), `uibuttongroup` (App Building), `uicheckbox` (App Building), `uiconfirm` (App Building), `uicontrol` (App Building), `uidatepicker` (App Building), `uidropdown` (App Building), `uieditfield` (App Building), `uifigure` (App Building), `uigauge` (App Building), `uigetdir` (App Building), `uigetfile` (App Building), `uigetpref` (App Building), `uigridlayout` (App Building), `uihtml` (App Building), `uihyperlink` (App Building), `uiimage` (App Building), `uiknob` (App Building), `uilabel` (App Building), `uilamp` (App Building), `uilistbox` (App Building), `uiopen` (App Building), `uipanel` (App Building), `uiprogressdlg` (App Building), `uipushtool` (App Building), `uiputfile` (App Building), `uiradiobutton` (App Building), `uiresume` (App Building), `uisave` (App Building), `uisetcolor` (App Building), `uisetfont` (App Building), `uisetpref` (App Building), `uislider` (App Building), `uispinner` (App Building), `uistack` (App Building), `uistyle` (App Building), `uiswitch` (App Building), `uitab` (App Building), `uitabgroup` (App Building), `uitable` (App Building), `uitextarea` (App Building), `uitogglebutton` (App Building), `uitoggletool` (App Building), `uitoolbar` (App Building), `uitree` (App Building), `uitreenode` (App Building), `uiwait` (App Building), `waitbar` (App Building), `waitfor` (App Building), `waitforbuttonpress` (App Building), `warndlg` (App Building)

## MATLAB > App Building > Develop Apps Using App Designer

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 1.

Out of scope: `App Designer` (App Building)

## MATLAB > App Building > Develop Live Editor Tasks

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 2.

Out of scope: `matlab.task.configureMetadata` (App Building), `matlab.task.removeMetadata` (App Building)

## MATLAB > App Building > Migrate GUIDE Apps

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 2.

Out of scope: `appmigration.migrateGUIDEApp` (App Building), `guide` (App Building)

## MATLAB > App Building > Package and Share Apps

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 6.

Out of scope: `matlab.apputil.create` (App Building), `matlab.apputil.getInstalledAppInfo` (App Building), `matlab.apputil.install` (App Building), `matlab.apputil.package` (App Building), `matlab.apputil.run` (App Building), `matlab.apputil.uninstall` (App Building)

## MATLAB > Data Import and Analysis > Data Import and Export

Implemented: 4. To implement: 97. In a branch: 0. Out of scope: 495.

To implement:

- [ ] `audioinfo`
- [ ] `audioread`
- [ ] `audiowrite`
- [ ] `clearvars`
- [ ] `csvread`
- [ ] `csvwrite`
- [ ] `daqread`
- [ ] `delimitedTextImportOptions`
- [ ] `dlmread`
- [ ] `dlmwrite`
- [ ] `evaluate`
- [ ] `fclose`
- [ ] `feof`
- [ ] `ferror`
- [ ] `fgetl`
- [ ] `fgets`
- [ ] `fileread`
- [ ] `fixedWidthImportOptions`
- [ ] `fopen`
- [ ] `formattedDisplayText`
- [ ] `fread`
- [ ] `frewind`
- [ ] `fscanf`
- [ ] `fseek`
- [ ] `ftell`
- [ ] `fwrite`
- [ ] `get`
- [ ] `getFileFormats`
- [ ] `getProfiles`
- [ ] `htmlImportOptions`
- [ ] `imfinfo`
- [ ] `importdata`
- [ ] `importNode`
- [ ] `imread`
- [ ] `imwrite`
- [ ] `jsondecode`
- [ ] `jsonencode`
- [ ] `lin2mu`
- [ ] `load`
- [ ] `matfile`
- [ ] `mu2lin`
- [ ] `open`
- [ ] `openvar`
- [ ] `read`
- [ ] `readcell`
- [ ] `readlines`
- [ ] `readmatrix`
- [ ] `readstruct`
- [ ] `readvars`
- [ ] `save`
- [ ] `set`
- [ ] `sheetnames`
- [ ] `textread`
- [ ] `textscan`
- [ ] `Tiff.close`
- [ ] `Tiff.computeStrip`
- [ ] `Tiff.computeTile`
- [ ] `Tiff.currentDirectory`
- [ ] `Tiff.getTag`
- [ ] `Tiff.getTagNames`
- [ ] `Tiff.getVersion`
- [ ] `Tiff.isTiled`
- [ ] `Tiff.lastDirectory`
- [ ] `Tiff.nextDirectory`
- [ ] `Tiff.numberOfStrips`
- [ ] `Tiff.numberOfTiles`
- [ ] `Tiff.read`
- [ ] `Tiff.readEncodedStrip`
- [ ] `Tiff.readEncodedTile`
- [ ] `Tiff.readRGBAImage`
- [ ] `Tiff.readRGBAStrip`
- [ ] `Tiff.readRGBATile`
- [ ] `Tiff.rewriteDirectory`
- [ ] `Tiff.setDirectory`
- [ ] `Tiff.setSubDirectory`
- [ ] `Tiff.setTag`
- [ ] `Tiff.write`
- [ ] `Tiff.writeDirectory`
- [ ] `Tiff.writeEncodedStrip`
- [ ] `Tiff.writeEncodedTile`
- [ ] `transform`
- [ ] `transformToString`
- [ ] `type`
- [ ] `who`
- [ ] `wordDocumentImportOptions`
- [ ] `workspace`
- [ ] `writecell`
- [ ] `writelines`
- [ ] `writematrix`
- [ ] `writestruct`
- [ ] `xlsfinfo`
- [ ] `xlsread`
- [ ] `xlswrite`
- [ ] `xmlImportOptions`
- [ ] `xmlread`
- [ ] `xmlwrite`
- [ ] `xslt`

Implemented: `clear`, `close`, `disp`, `fprintf`

Out of scope: `accellog` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `angvellog` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `ascii` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `Attribute (H5A)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `audiodevinfo` (Audio and video devices), `audiodevreset` (Audio and video devices), `audioplayer` (Audio and video devices), `audiorecorder` (Audio and video devices), `aviinfo` (Audio and video devices), `beep` (Audio and video devices), `binary` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `ble` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `blelist` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `bluetooth` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `bluetoothlist` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `camera` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `cdfepoch` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdfinfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.breakdownTT2000` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.close` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.closeVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.computeEpoch` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.computeEpoch16` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.computeTT2000` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.create` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.createAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.createVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.delete` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.deleteAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.deleteAttrEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.deleteAttrgEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.deleteVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.deleteVarRecords` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.epoch16Breakdown` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.epochBreakdown` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getAttrEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getAttrgEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getAttrMaxEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getAttrMaxgEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getAttrName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getAttrNum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getAttrScope` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getChecksum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getCompression` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getCompressionCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getConstantNames` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getConstantValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getCopyright` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getFileBackward` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getFormat` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getLibraryCopyright` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getLibraryVersion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getMajority` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getNumAttrEntries` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getNumAttrgEntries` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getNumAttributes` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getNumgAttributes` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getReadOnlyMode` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getStageCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getValidate` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarAllocRecords` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarBlockingFactor` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarCompression` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarData` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarMaxAllocRecNum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarMaxWrittenRecNum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarNum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarNumRecsWritten` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarPadValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarRecordData` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarReservePercent` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarsMaxWrittenRecNum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVarSparseRecords` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.getVersion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.hyperGetVarData` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.hyperPutVarData` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.inquire` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.inquireAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.inquireAttrEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.inquireAttrgEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.inquireVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.open` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.putAttrEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.putAttrgEntry` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.putVarData` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.putVarRecordData` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.renameAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.renameVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setChecksum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setCompression` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setCompressionCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setFileBackward` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setFormat` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setMajority` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setReadOnlyMode` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setStageCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setValidate` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarAllocBlockRecords` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarBlockingFactor` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarCompression` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarInitialRecs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarPadValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarReservePercent` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarsCacheSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdflib.setVarSparseRecords` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdfread` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `cdfwrite` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `characteristic` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `closeFile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `configureCallback` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `configureTerminator` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `copyHDU` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `createFile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `createImg` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `createTbl` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `Dataset (H5D)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `Dataspace (H5S)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `Datatype (H5T)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `deleteCol` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `deleteFile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `deleteHDU` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `deleteKey` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `deleteRecord` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `deleteRows` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `descriptor` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `Dimension Scale (H5DS)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `discardlogs` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `echotcpip` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `Error (H5E)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `fclose (serial)` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `fgetl (serial)` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `fgets (serial)` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `File (H5F)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `fileMode` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `fileName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `Filters and Compression (H5Z)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `fitsdisp` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `fitsinfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `fitsread` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `fitswrite` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `flush` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `fopen (serial)` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `fprintf (serial)` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `fread (serial)` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `fscanf (serial)` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `ftp` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `fwrite (serial)` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `getAColParms` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getaudiodata` (Audio and video devices), `getBColParms` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getColName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getColType` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getConstantValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getEqColType` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getHdrSpace` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getHDUnum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getHDUtype` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getImgSize` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getImgType` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getNumCols` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getNumHDUs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getNumRows` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getOpenFiles` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `getpinstatus` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `getplayer` (Audio and video devices), `getVersion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `Group (H5G)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `h5create` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `h5disp` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `h5info` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `h5read` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `h5readatt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `h5write` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `h5writeatt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hasFrame` (Audio and video devices), `hdf5info` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdf5read` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdf5write` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfan` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfdf24` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfdfr8` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfh` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfhd` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfhe` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfhx` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfinfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfml` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfpt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfread` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdftool` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfv` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfvf` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfvh` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `hdfvs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `Identifier (H5I)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `imgCompress` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `insertATbl` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `insertBTbl` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `insertCol` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `insertImg` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `insertRows` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `instrcallback` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `instrfind` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `instrfindall` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `isCompressedImg` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `isplaying` (Audio and video devices), `isrecording` (Audio and video devices), `Library (H5)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `Link (H5L)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `magfieldlog` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `MATLAB (H5ML)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.attrInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.close` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.create` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.dimInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.endAccess` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.fileInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.findAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getCal` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getChunkInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getCompInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getDataStrs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getDimID` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getDimScale` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getDimStrs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getFilename` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getFillValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.getRange` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.idToRef` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.idType` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.isCoordVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.isRecord` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.nameToIndex` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.nameToIndices` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.readAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.readChunk` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.readData` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.refToIndex` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.select` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setCal` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setChunk` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setCompress` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setDataStrs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setDimName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setDimScale` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setDimStrs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setExternalFile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setFillMode` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setFillValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setNBitDataSet` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.setRange` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.start` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.writeChunk` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdf4.sd.writeData` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.attach` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.close` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.compInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.create` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defBoxRegion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defComp` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defDim` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defField` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defOrigin` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defPixReg` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defProj` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defTile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.defVrtRegion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.detach` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.dimInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.extractRegion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.fieldInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.getFillValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.getPixels` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.getPixValues` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.gridInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.ij2ll` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.inqAttrs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.inqDims` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.inqFields` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.inqGrid` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.interpolate` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.ll2ij` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.nEntries` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.open` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.originInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.pixRegInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.projInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.readAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.readBlkSomOffset` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.readField` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.readTile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.regionInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.setFillValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.setTileComp` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.sphereCodeToName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.sphereNameToCode` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.tileInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.writeAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.writeBlkSomOffset` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.writeField` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.gd.writeTile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.attach` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.close` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.compInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.create` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.defBoxRegion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.defComp` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.defDataField` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.defDim` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.defDimMap` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.defGeoField` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.defTimePeriod` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.defVrtRegion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.detach` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.dimInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.extractPeriod` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.extractRegion` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.fieldInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.geoMapInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.getFillValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.idxMapInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.inqAttrs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.inqDataFields` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.inqDims` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.inqGeoFields` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.inqIdxMaps` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.inqMaps` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.inqSwath` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.mapInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.nEntries` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.open` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.periodInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.readAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.readField` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.regionInfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.setFillValue` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.writeAttr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `matlab.io.hdfeos.sw.writeField` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `mget` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `mmfileinfo` (Audio and video devices), `mobiledev` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `mobiledevlist` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `movAbsHDU` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `movNamHDU` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `movRelHDU` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `mput` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `multibandread` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `multibandwrite` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `nccreate` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `ncdisp` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `ncinfo` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `ncread` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `ncreadatt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `ncwrite` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `ncwriteatt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `ncwriteschema` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.abort` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.close` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.copyAtt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.create` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.defDim` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.defGrp` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.defVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.defVarChunking` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.defVarDeflate` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.defVarFill` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.defVarFletcher32` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.defVlen` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.delAtt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.endDef` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.getAtt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.getChunkCache` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.getConstant` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.getConstantNames` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.getVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inq` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqAtt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqAttID` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqAttName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqDim` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqDimID` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqDimIDs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqFormat` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqGrpName` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqGrpNameFull` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqGrpParent` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqGrps` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqLibVers` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqNcid` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqUnlimDims` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqUserType` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqVarChunking` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqVarDeflate` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqVarFill` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqVarFletcher32` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqVarID` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqVarIDs` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.inqVlen` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.open` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.putAtt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.putVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.reDef` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.renameAtt` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.renameDim` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.renameVar` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.setChunkCache` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.setDefaultFormat` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.setFill` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `netcdf.sync` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `Object (H5O)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `openDiskFile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `openFile` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `orientlog` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `pause` (Audio and video devices), `play` (Audio and video devices), `playblocking` (Audio and video devices), `poslog` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `Property (H5P)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readasync` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `readATblHdr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readAudio` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `readBTblHdr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readCard` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readCol` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readFrame` (Audio and video devices), `readImg` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readKey` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readKeyCmplx` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readKeyDbl` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readKeyLongLong` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readKeyLongStr` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readKeyUnit` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `readline` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `readMobileSensorData` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `readRecord` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `record` (Audio and video devices), `recordblocking` (Audio and video devices), `Reference (H5R)` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `rename` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `resume` (Audio and video devices), `sendmail` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `serial` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `serial Properties` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `serialbreak` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `seriallist` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `serialport` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `serialportlist` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `setBscale` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `setCompressionType` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `setDTR` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `setHCompScale` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `setHCompSmooth` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `setRTS` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `setTileDim` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `setTscale` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `sftp` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `snapshot` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `sound` (Audio and video devices), `soundsc` (Audio and video devices), `stopasync` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `subscribe` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `tcpclient` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `thingSpeakRead` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `thingSpeakWrite` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `todatenum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `unsubscribe` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `urlread` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `urlwrite` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `VideoReader` (Audio and video devices), `VideoWriter` (Audio and video devices), `web` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `weboptions` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `webread` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `websave` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `webwrite` (Data Import and Analysis > Data Import and Export > Web Access and Streaming), `write` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `writeChecksum` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `writeCol` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `writeComment` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `writeDate` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `writeHistory` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `writeImg` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `writeKey` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `writeKeyUnit` (Data Import and Analysis > Data Import and Export > Standard File Formats > Scientific Data), `writeline` (Data Import and Analysis > Data Import and Export > Hardware and Network Communication), `writeVideo` (Audio and video devices)

## MATLAB > Data Import and Analysis > Descriptive Statistics

Implemented: 19. To implement: 2. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `kde`
- [ ] `rms`

Implemented: `bounds`, `corrcoef`, `cov`, `cummax`, `cummin`, `mape`, `maxk`, `mink`, `mode`, `movmax`, `movmin`, `movprod`, `movstd`, `movvar`, `prctile`, `quantile`, `rmse`, `xcorr`, `xcov`

## MATLAB > Data Import and Analysis > Large Files and Big Data

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 43.

Out of scope: `add` (Data Import and Analysis > Large Files and Big Data), `addmulti` (Data Import and Analysis > Large Files and Big Data), `arrayDatastore` (Data Import and Analysis > Large Files and Big Data), `classUnderlying` (Data Import and Analysis > Large Files and Big Data), `CombinedDatastore` (Data Import and Analysis > Large Files and Big Data), `countEachLabel` (Data Import and Analysis > Large Files and Big Data), `datastore` (Data Import and Analysis > Large Files and Big Data), `fileDatastore` (Data Import and Analysis > Large Files and Big Data), `gather` (Data Import and Analysis > Large Files and Big Data), `gcmr` (Data Import and Analysis > Large Files and Big Data), `getnext` (Data Import and Analysis > Large Files and Big Data), `hasdata` (Data Import and Analysis > Large Files and Big Data), `hasnext` (Data Import and Analysis > Large Files and Big Data), `imageDatastore` (Data Import and Analysis > Large Files and Big Data), `isaUnderlying` (Data Import and Analysis > Large Files and Big Data), `isPartitionable` (Data Import and Analysis > Large Files and Big Data), `isShuffleable` (Data Import and Analysis > Large Files and Big Data), `isSubsettable` (Data Import and Analysis > Large Files and Big Data), `istall` (Data Import and Analysis > Large Files and Big Data), `KeyValueDatastore` (Data Import and Analysis > Large Files and Big Data), `mapreduce` (Data Import and Analysis > Large Files and Big Data), `mapreducer` (Data Import and Analysis > Large Files and Big Data), `matlab.tall.blockMovingWindow` (Data Import and Analysis > Large Files and Big Data), `matlab.tall.movingWindow` (Data Import and Analysis > Large Files and Big Data), `matlab.tall.reduce` (Data Import and Analysis > Large Files and Big Data), `matlab.tall.transform` (Data Import and Analysis > Large Files and Big Data), `memmapfile` (Data Import and Analysis > Large Files and Big Data), `numpartitions` (Data Import and Analysis > Large Files and Big Data), `parquetDatastore` (Data Import and Analysis > Large Files and Big Data), `partition` (Data Import and Analysis > Large Files and Big Data), `readall` (Data Import and Analysis > Large Files and Big Data), `readimage` (Data Import and Analysis > Large Files and Big Data), `SequentialDatastore` (Data Import and Analysis > Large Files and Big Data), `shuffle` (Data Import and Analysis > Large Files and Big Data), `splitEachLabel` (Data Import and Analysis > Large Files and Big Data), `spreadsheetDatastore` (Data Import and Analysis > Large Files and Big Data), `subset` (Data Import and Analysis > Large Files and Big Data), `tabularTextDatastore` (Data Import and Analysis > Large Files and Big Data), `tall` (Data Import and Analysis > Large Files and Big Data), `TallDatastore` (Data Import and Analysis > Large Files and Big Data), `tallrng` (Data Import and Analysis > Large Files and Big Data), `TransformedDatastore` (Data Import and Analysis > Large Files and Big Data), `writeall` (Data Import and Analysis > Large Files and Big Data)

## MATLAB > Data Import and Analysis > Preprocessing Data

Implemented: 19. To implement: 0. In a branch: 0. Out of scope: 0.

Implemented: `accumarray`, `fillmissing2`, `filloutliers`, `histcounts`, `histcounts2`, `ischange`, `islocalmax`, `islocalmin`, `isoutlier`, `missing`, `movmad`, `movmean`, `movmedian`, `normalize`, `rescale`, `rmoutliers`, `smoothdata`, `smoothdata2`, `trenddecomp`

## MATLAB > Data Import and Analysis > Visual Exploration

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 30.

Out of scope: `addToolbarExplorationButtons` (Data Import and Analysis > Visual Exploration), `axtoolbar` (Data Import and Analysis > Visual Exploration), `axtoolbarbtn` (Data Import and Analysis > Visual Exploration), `brush` (Data Import and Analysis > Visual Exploration), `datacursormode` (Data Import and Analysis > Visual Exploration), `dataTipInteraction` (Data Import and Analysis > Visual Exploration), `dataTipTextRow` (Data Import and Analysis > Visual Exploration), `disableDefaultInteractivity` (Data Import and Analysis > Visual Exploration), `editInteraction` (Data Import and Analysis > Visual Exploration), `enableDefaultInteractivity` (Data Import and Analysis > Visual Exploration), `enableLegacyExplorationModes` (Data Import and Analysis > Visual Exploration), `figurepalette` (Data Import and Analysis > Visual Exploration), `linkaxes` (Data Import and Analysis > Visual Exploration), `linkdata` (Data Import and Analysis > Visual Exploration), `linkprop` (Data Import and Analysis > Visual Exploration), `pan` (Data Import and Analysis > Visual Exploration), `panInteraction` (Data Import and Analysis > Visual Exploration), `plotbrowser` (Data Import and Analysis > Visual Exploration), `plotedit` (Data Import and Analysis > Visual Exploration), `plottools` (Data Import and Analysis > Visual Exploration), `propedit` (Data Import and Analysis > Visual Exploration), `propertyeditor` (Data Import and Analysis > Visual Exploration), `regionZoomInteraction` (Data Import and Analysis > Visual Exploration), `removeToolbarExplorationButtons` (Data Import and Analysis > Visual Exploration), `rotate3d` (Data Import and Analysis > Visual Exploration), `rotateInteraction` (Data Import and Analysis > Visual Exploration), `rulerPanInteraction` (Data Import and Analysis > Visual Exploration), `showplottool` (Data Import and Analysis > Visual Exploration), `zoom` (Data Import and Analysis > Visual Exploration), `zoomInteraction` (Data Import and Analysis > Visual Exploration)

## MATLAB > Environment and Settings > Add-Ons

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 12.

Out of scope: `matlab.addons.disableAddon` (Environment and Settings > Add-Ons), `matlab.addons.enableAddon` (Environment and Settings > Add-Ons), `matlab.addons.install` (Environment and Settings > Add-Ons), `matlab.addons.installedAddons` (Environment and Settings > Add-Ons), `matlab.addons.isAddonEnabled` (Environment and Settings > Add-Ons), `matlab.addons.uninstall` (Environment and Settings > Add-Ons), `matlabshared.supportpkg.checkForUpdate` (Environment and Settings > Add-Ons), `matlabshared.supportpkg.getInstalled` (Environment and Settings > Add-Ons), `matlabshared.supportpkg.getSupportPackageRoot` (Environment and Settings > Add-Ons), `matlabshared.supportpkg.setSupportPackageRoot` (Environment and Settings > Add-Ons), `supportPackageInstaller` (Environment and Settings > Add-Ons), `upgradePreviouslyInstalledSupportPackages` (Environment and Settings > Add-Ons)

## MATLAB > Environment and Settings > Desktop

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 1.

Out of scope: `prefdir` (Environment and Settings > Desktop)

## MATLAB > Environment and Settings > Help and Support

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 13.

Out of scope: `demo` (Environment and Settings > Help and Support), `doc` (Environment and Settings > Help and Support), `docsearch` (Environment and Settings > Help and Support), `echodemo` (Environment and Settings > Help and Support), `help` (Environment and Settings > Help and Support), `helpbrowser` (Environment and Settings > Help and Support), `helpdesk` (Environment and Settings > Help and Support), `helpwin` (Environment and Settings > Help and Support), `info` (Environment and Settings > Help and Support), `lookfor` (Environment and Settings > Help and Support), `openExample` (Environment and Settings > Help and Support), `support` (Environment and Settings > Help and Support), `whatsnew` (Environment and Settings > Help and Support)

## MATLAB > Environment and Settings > Platform and License

Implemented: 0. To implement: 13. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `computer`
- [ ] `ismac`
- [ ] `isMATLABReleaseOlderThan`
- [ ] `ispc`
- [ ] `isstudent`
- [ ] `isunix`
- [ ] `javachk`
- [ ] `license`
- [ ] `matlabRelease`
- [ ] `usejava`
- [ ] `ver`
- [ ] `verLessThan`
- [ ] `version`

## MATLAB > Environment and Settings > Startup and Shutdown

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 9.

Out of scope: `batchStartupOptionUsed` (Environment and Settings > Startup and Shutdown), `exit` (Environment and Settings > Startup and Shutdown), `finish` (Environment and Settings > Startup and Shutdown), `matlab (Linux)` (Environment and Settings > Startup and Shutdown), `matlab (macOS)` (Environment and Settings > Startup and Shutdown), `matlab (Windows)` (Environment and Settings > Startup and Shutdown), `matlabrc` (Environment and Settings > Startup and Shutdown), `quit` (Environment and Settings > Startup and Shutdown), `startup` (Environment and Settings > Startup and Shutdown)

## MATLAB > Environment and Settings > System Commands

Implemented: 0. To implement: 38. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addGroup`
- [ ] `addpref`
- [ ] `addSetting`
- [ ] `clearInstallationValue`
- [ ] `clearPersonalValue`
- [ ] `clearTemporaryValue`
- [ ] `clipboard`
- [ ] `cmdsep`
- [ ] `dos`
- [ ] `getenv`
- [ ] `getpref`
- [ ] `hasFactoryValue`
- [ ] `hasGroup`
- [ ] `hasInstallationValue`
- [ ] `hasPersonalValue`
- [ ] `hasSetting`
- [ ] `hasTemporaryValue`
- [ ] `isenv`
- [ ] `ispref`
- [ ] `loadenv`
- [ ] `matlab.io.saveVariablesToScript`
- [ ] `matlab.settings.mustBeIntegerScalar`
- [ ] `matlab.settings.mustBeLogicalScalar`
- [ ] `matlab.settings.mustBeNumericScalar`
- [ ] `matlab.settings.mustBeStringScalar`
- [ ] `perl`
- [ ] `removeGroup`
- [ ] `removeSetting`
- [ ] `rmpref`
- [ ] `setenv`
- [ ] `setpref`
- [ ] `Setting`
- [ ] `settings`
- [ ] `SettingsGroup`
- [ ] `system`
- [ ] `unix`
- [ ] `unsetenv`
- [ ] `winqueryreg`

## MATLAB > External Language Interfaces > .NET with MATLAB

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 21.

Out of scope: `BeginInvoke` (External Language Interfaces), `bitnot` (External Language Interfaces), `Combine` (External Language Interfaces), `dotnetenv` (External Language Interfaces), `enableNETfromNetworkDrive` (External Language Interfaces), `EndInvoke` (External Language Interfaces), `matlab.engine.typedinterface.generateCSharp` (External Language Interfaces), `NET` (External Language Interfaces), `NET.addAssembly` (External Language Interfaces), `NET.convertArray` (External Language Interfaces), `NET.createArray` (External Language Interfaces), `NET.createDictionary` (External Language Interfaces), `NET.createGeneric` (External Language Interfaces), `NET.disableAutoRelease` (External Language Interfaces), `NET.enableAutoRelease` (External Language Interfaces), `NET.interfaceView` (External Language Interfaces), `NET.invokeGenericMethod` (External Language Interfaces), `NET.isNETSupported` (External Language Interfaces), `NET.setStaticProperty` (External Language Interfaces), `Remove` (External Language Interfaces), `RemoveAll` (External Language Interfaces)

## MATLAB > External Language Interfaces > C with MATLAB

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 19.

Out of scope: `calllib` (External Language Interfaces), `dbmex` (External Language Interfaces), `lib.pointer.disp` (External Language Interfaces), `lib.pointer.isNull` (External Language Interfaces), `lib.pointer.plus` (External Language Interfaces), `lib.pointer.reshape` (External Language Interfaces), `lib.pointer.setdatatype` (External Language Interfaces), `libfunctions` (External Language Interfaces), `libfunctionsview` (External Language Interfaces), `libisloaded` (External Language Interfaces), `libpointer` (External Language Interfaces), `libstruct` (External Language Interfaces), `loadlibrary` (External Language Interfaces), `mex` (External Language Interfaces), `mex.getCompilerConfigurations` (External Language Interfaces), `mexErrMsgTxt` (External Language Interfaces), `mexSetTrapFlag` (External Language Interfaces), `mexWarnMsgTxt` (External Language Interfaces), `unloadlibrary` (External Language Interfaces)

## MATLAB > External Language Interfaces > C++ with MATLAB

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 26.

Out of scope: `clibArray` (External Language Interfaces), `clibConfiguration` (External Language Interfaces), `clibConvertArray` (External Language Interfaces), `clibgen.buildInterface` (External Language Interfaces), `clibgen.generateLibraryDefinition` (External Language Interfaces), `clibIsNull` (External Language Interfaces), `clibIsReadOnly` (External Language Interfaces), `clibPublishInterfaceWorkflow` (External Language Interfaces), `CLibraryConfiguration` (External Language Interfaces), `clibRelease` (External Language Interfaces), `matlab.engine.engineName` (External Language Interfaces), `matlab.engine.isEngineShared` (External Language Interfaces), `matlab.engine.shareEngine` (External Language Interfaces), `matlab.engine.typedinterface.generateCPP` (External Language Interfaces), `matlab::engine::connectMATLAB` (External Language Interfaces), `matlab::engine::connectMATLABAsync` (External Language Interfaces), `matlab::engine::convertUTF16StringToUTF8String` (External Language Interfaces), `matlab::engine::convertUTF8StringToUTF16String` (External Language Interfaces), `matlab::engine::findMATLAB` (External Language Interfaces), `matlab::engine::findMATLABAsync` (External Language Interfaces), `matlab::engine::startMATLAB` (External Language Interfaces), `matlab::engine::startMATLABAsync` (External Language Interfaces), `matlab::engine::terminateEngineClient` (External Language Interfaces), `mexext` (External Language Interfaces), `underlyingValue` (External Language Interfaces), `unload` (External Language Interfaces)

## MATLAB > External Language Interfaces > COM with MATLAB

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 34.

Out of scope: `actxcontrollist` (External Language Interfaces), `actxcontrolselect` (External Language Interfaces), `actxGetRunningServer` (External Language Interfaces), `actxserver` (External Language Interfaces), `addproperty` (External Language Interfaces), `comserver` (External Language Interfaces), `deleteproperty` (External Language Interfaces), `enableservice` (External Language Interfaces), `eventlisteners` (External Language Interfaces), `events (COM)` (External Language Interfaces), `Execute` (External Language Interfaces), `Feval` (External Language Interfaces), `GetCharArray` (External Language Interfaces), `GetFullMatrix` (External Language Interfaces), `GetVariable` (External Language Interfaces), `GetWorkspaceData` (External Language Interfaces), `invoke` (External Language Interfaces), `iscom` (External Language Interfaces), `isevent` (External Language Interfaces), `isinterface` (External Language Interfaces), `load (COM)` (External Language Interfaces), `MaximizeCommandWindow` (External Language Interfaces), `methodsview` (External Language Interfaces), `MinimizeCommandWindow` (External Language Interfaces), `propedit (COM)` (External Language Interfaces), `PutCharArray` (External Language Interfaces), `PutFullMatrix` (External Language Interfaces), `PutWorkspaceData` (External Language Interfaces), `Quit` (External Language Interfaces), `registerevent` (External Language Interfaces), `regmatlabserver` (External Language Interfaces), `save (COM)` (External Language Interfaces), `unregisterallevents` (External Language Interfaces), `unregisterevent` (External Language Interfaces)

## MATLAB > External Language Interfaces > Java with MATLAB

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 10.

Out of scope: `javaaddpath` (External Language Interfaces), `javaArray` (External Language Interfaces), `javaclasspath` (External Language Interfaces), `javaMethod` (External Language Interfaces), `javaMethodEDT` (External Language Interfaces), `javaObject` (External Language Interfaces), `javaObjectEDT` (External Language Interfaces), `javarmpath` (External Language Interfaces), `jenv` (External Language Interfaces), `matlab_jenv` (External Language Interfaces)

## MATLAB > External Language Interfaces > Python with MATLAB

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 15.

Out of scope: `matlab.engine.connect_matlab` (External Language Interfaces), `matlab.engine.find_matlab` (External Language Interfaces), `matlab.engine.FutureResult.cancel` (External Language Interfaces), `matlab.engine.FutureResult.cancelled` (External Language Interfaces), `matlab.engine.FutureResult.done` (External Language Interfaces), `matlab.engine.FutureResult.result` (External Language Interfaces), `matlab.engine.start_matlab` (External Language Interfaces), `matlab.exception.PyException` (External Language Interfaces), `pyargs` (External Language Interfaces), `pyenv` (External Language Interfaces), `pyrun` (External Language Interfaces), `pyrunfile` (External Language Interfaces), `PythonEnvironment` (External Language Interfaces), `pyversion` (External Language Interfaces), `terminate` (External Language Interfaces)

## MATLAB > External Language Interfaces > Web Services with MATLAB

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 98.

Out of scope: `addFields` (External Language Interfaces), `callSoapService` (External Language Interfaces), `changeFields` (External Language Interfaces), `createClassFromWsdl` (External Language Interfaces), `createSoapMessage` (External Language Interfaces), `getFields` (External Language Interfaces), `matlab.net.base64decode` (External Language Interfaces), `matlab.net.base64encode` (External Language Interfaces), `matlab.net.http.AuthInfo.getParameter` (External Language Interfaces), `matlab.net.http.AuthInfo.removeParameter` (External Language Interfaces), `matlab.net.http.AuthInfo.setParameter` (External Language Interfaces), `matlab.net.http.CookieInfo.collectFromLog` (External Language Interfaces), `matlab.net.http.field.GenericField.getParameter` (External Language Interfaces), `matlab.net.http.field.GenericField.removeParameter` (External Language Interfaces), `matlab.net.http.field.GenericField.setParameter` (External Language Interfaces), `matlab.net.http.field.SetCookieField.convert` (External Language Interfaces), `matlab.net.http.HeaderField.convert` (External Language Interfaces), `matlab.net.http.HeaderField.convertLike` (External Language Interfaces), `matlab.net.http.HeaderField.displaySubclasses` (External Language Interfaces), `matlab.net.http.HeaderField.eq` (External Language Interfaces), `matlab.net.http.HeaderField.parse` (External Language Interfaces), `matlab.net.http.io.BinaryConsumer.putData` (External Language Interfaces), `matlab.net.http.io.BinaryConsumer.start` (External Language Interfaces), `matlab.net.http.io.ContentConsumer.delegateTo` (External Language Interfaces), `matlab.net.http.io.ContentConsumer.initialize` (External Language Interfaces), `matlab.net.http.io.ContentConsumer.putData` (External Language Interfaces), `matlab.net.http.io.ContentConsumer.start` (External Language Interfaces), `matlab.net.http.io.ContentProvider.complete` (External Language Interfaces), `matlab.net.http.io.ContentProvider.delegateTo` (External Language Interfaces), `matlab.net.http.io.ContentProvider.expectedContentLength` (External Language Interfaces), `matlab.net.http.io.ContentProvider.getData` (External Language Interfaces), `matlab.net.http.io.ContentProvider.preferredBufferSize` (External Language Interfaces), `matlab.net.http.io.ContentProvider.restartable` (External Language Interfaces), `matlab.net.http.io.ContentProvider.reusable` (External Language Interfaces), `matlab.net.http.io.ContentProvider.show` (External Language Interfaces), `matlab.net.http.io.ContentProvider.start` (External Language Interfaces), `matlab.net.http.io.ContentProvider.string` (External Language Interfaces), `matlab.net.http.io.FileConsumer.putData` (External Language Interfaces), `matlab.net.http.io.FileConsumer.start` (External Language Interfaces), `matlab.net.http.io.FileProvider.getData` (External Language Interfaces), `matlab.net.http.io.FileProvider.string` (External Language Interfaces), `matlab.net.http.io.FormProvider.string` (External Language Interfaces), `matlab.net.http.io.GenericConsumer.putData` (External Language Interfaces), `matlab.net.http.io.GenericConsumer.start` (External Language Interfaces), `matlab.net.http.io.GenericProvider.expectedContentLength` (External Language Interfaces), `matlab.net.http.io.GenericProvider.getData` (External Language Interfaces), `matlab.net.http.io.GenericProvider.preferredBufferSize` (External Language Interfaces), `matlab.net.http.io.GenericProvider.restartable` (External Language Interfaces), `matlab.net.http.io.GenericProvider.reusable` (External Language Interfaces), `matlab.net.http.io.ImageConsumer.initialize` (External Language Interfaces), `matlab.net.http.io.ImageConsumer.putData` (External Language Interfaces), `matlab.net.http.io.ImageProvider.complete` (External Language Interfaces), `matlab.net.http.io.ImageProvider.restartable` (External Language Interfaces), `matlab.net.http.io.ImageProvider.reusable` (External Language Interfaces), `matlab.net.http.io.ImageProvider.start` (External Language Interfaces), `matlab.net.http.io.JSONConsumer.initialize` (External Language Interfaces), `matlab.net.http.io.JSONConsumer.putData` (External Language Interfaces), `matlab.net.http.io.JSONProvider.complete` (External Language Interfaces), `matlab.net.http.io.JSONProvider.start` (External Language Interfaces), `matlab.net.http.io.MultipartConsumer.initialize` (External Language Interfaces), `matlab.net.http.io.MultipartConsumer.putData` (External Language Interfaces), `matlab.net.http.io.MultipartConsumer.start` (External Language Interfaces), `matlab.net.http.io.MultipartProvider.complete` (External Language Interfaces), `matlab.net.http.io.MultipartProvider.getData` (External Language Interfaces), `matlab.net.http.io.MultipartProvider.start` (External Language Interfaces), `matlab.net.http.io.StringConsumer.convert` (External Language Interfaces), `matlab.net.http.io.StringConsumer.initialize` (External Language Interfaces), `matlab.net.http.io.StringConsumer.putData` (External Language Interfaces), `matlab.net.http.io.StringConsumer.start` (External Language Interfaces), `matlab.net.http.io.StringProvider.complete` (External Language Interfaces), `matlab.net.http.io.StringProvider.getData` (External Language Interfaces), `matlab.net.http.io.StringProvider.start` (External Language Interfaces), `matlab.net.http.io.StringProvider.string` (External Language Interfaces), `matlab.net.http.LogRecord.show` (External Language Interfaces), `matlab.net.http.MediaType.getParameter` (External Language Interfaces), `matlab.net.http.MediaType.setParameter` (External Language Interfaces), `matlab.net.http.Message.addFields` (External Language Interfaces), `matlab.net.http.Message.changeFields` (External Language Interfaces), `matlab.net.http.Message.getFields` (External Language Interfaces), `matlab.net.http.Message.removeFields` (External Language Interfaces), `matlab.net.http.Message.replaceFields` (External Language Interfaces), `matlab.net.http.Message.show` (External Language Interfaces), `matlab.net.http.MessageBody.show` (External Language Interfaces), `matlab.net.http.ProgressMonitor.done` (External Language Interfaces), `matlab.net.http.RequestMessage.complete` (External Language Interfaces), `matlab.net.http.RequestMessage.send` (External Language Interfaces), `matlab.net.http.ResponseMessage.complete` (External Language Interfaces), `matlab.net.http.StatusClass.getReasonPhrase` (External Language Interfaces), `matlab.net.http.StatusCode.fromValue` (External Language Interfaces), `matlab.net.http.StatusCode.getClass` (External Language Interfaces), `matlab.net.http.StatusCode.getReasonPhrase` (External Language Interfaces), `matlab.net.QueryParameter.char` (External Language Interfaces), `matlab.net.QueryParameter.string` (External Language Interfaces), `matlab.wsdl.createWSDLClient` (External Language Interfaces), `matlab.wsdl.setWSDLToolPath` (External Language Interfaces), `parseSoapResponse` (External Language Interfaces), `removeFields` (External Language Interfaces), `replaceFields` (External Language Interfaces)

## MATLAB > Graphics > 2-D and 3-D Plots

Implemented: 8. To implement: 140. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addpoints`
- [ ] `addToolbarMapButton`
- [ ] `animatedline`
- [ ] `bar3`
- [ ] `bar3h`
- [ ] `barh`
- [ ] `binscatter`
- [ ] `boxchart`
- [ ] `bubblechart`
- [ ] `bubblechart3`
- [ ] `bubblecloud`
- [ ] `bubblelegend`
- [ ] `bubblelim`
- [ ] `bubblesize`
- [ ] `clabel`
- [ ] `clearpoints`
- [ ] `comet`
- [ ] `comet3`
- [ ] `compass`
- [ ] `coneplot`
- [ ] `contour`
- [ ] `contour3`
- [ ] `contourc`
- [ ] `contourf`
- [ ] `contourslice`
- [ ] `curl`
- [ ] `cylinder`
- [ ] `divergence`
- [ ] `donutchart`
- [ ] `ellipsoid`
- [ ] `errorbar`
- [ ] `ezcontour`
- [ ] `ezcontourf`
- [ ] `ezmesh`
- [ ] `ezmeshc`
- [ ] `ezplot`
- [ ] `ezplot3`
- [ ] `ezpolar`
- [ ] `ezsurf`
- [ ] `ezsurfc`
- [ ] `fcontour`
- [ ] `feather`
- [ ] `fewerbins`
- [ ] `fill`
- [ ] `fill3`
- [ ] `fimplicit`
- [ ] `fimplicit3`
- [ ] `flow`
- [ ] `fmesh`
- [ ] `fplot3`
- [ ] `frame2im`
- [ ] `fsurf`
- [ ] `geoaxes`
- [ ] `geobasemap`
- [ ] `geobubble`
- [ ] `geodensityplot`
- [ ] `geolimits`
- [ ] `geoplot`
- [ ] `geoscatter`
- [ ] `geotickformat`
- [ ] `getframe`
- [ ] `getpoints`
- [ ] `heatmap`
- [ ] `hidden`
- [ ] `hist`
- [ ] `histogram2`
- [ ] `im2frame`
- [ ] `interpstreamspeed`
- [ ] `isocaps`
- [ ] `isocolors`
- [ ] `isonormals`
- [ ] `isosurface`
- [ ] `loglog`
- [ ] `mesh`
- [ ] `meshc`
- [ ] `meshz`
- [ ] `morebins`
- [ ] `movie`
- [ ] `parallelplot`
- [ ] `pareto`
- [ ] `pcolor`
- [ ] `peaks`
- [ ] `pie`
- [ ] `pie3`
- [ ] `piechart`
- [ ] `plot3`
- [ ] `plotmatrix`
- [ ] `polar`
- [ ] `polaraxes`
- [ ] `polarbubblechart`
- [ ] `polarhistogram`
- [ ] `polarplot`
- [ ] `polarscatter`
- [ ] `quiver`
- [ ] `quiver3`
- [ ] `reducepatch`
- [ ] `reducevolume`
- [ ] `refreshdata`
- [ ] `removeToolbarMapButton`
- [ ] `ribbon`
- [ ] `rlim`
- [ ] `rose`
- [ ] `rtickangle`
- [ ] `rtickformat`
- [ ] `rticklabels`
- [ ] `rticks`
- [ ] `scatter3`
- [ ] `scatterhistogram`
- [ ] `semilogx`
- [ ] `semilogy`
- [ ] `shrinkfaces`
- [ ] `slice`
- [ ] `smooth3`
- [ ] `sortx`
- [ ] `sorty`
- [ ] `sphere`
- [ ] `stem3`
- [ ] `stream2`
- [ ] `stream3`
- [ ] `streamline`
- [ ] `streamparticles`
- [ ] `streamribbon`
- [ ] `streamslice`
- [ ] `streamtube`
- [ ] `subvolume`
- [ ] `surf`
- [ ] `surf2patch`
- [ ] `surface`
- [ ] `surfc`
- [ ] `surfl`
- [ ] `surfnorm`
- [ ] `swarmchart`
- [ ] `swarmchart3`
- [ ] `thetalim`
- [ ] `thetatickformat`
- [ ] `thetaticklabels`
- [ ] `thetaticks`
- [ ] `volumebounds`
- [ ] `waterfall`
- [ ] `wordcloud`

Implemented: `bar`, `drawnow`, `fplot`, `histc`, `histogram`, `scatter`, `stairs`, `stem`

## MATLAB > Graphics > Formatting and Annotation

Implemented: 18. To implement: 106. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `abyss`
- [ ] `alim`
- [ ] `alpha`
- [ ] `alphamap`
- [ ] `annotation`
- [ ] `autumn`
- [ ] `axes`
- [ ] `bone`
- [ ] `brighten`
- [ ] `camdolly`
- [ ] `cameratoolbar`
- [ ] `camlight`
- [ ] `camlookat`
- [ ] `camorbit`
- [ ] `campan`
- [ ] `campos`
- [ ] `camproj`
- [ ] `camroll`
- [ ] `camtarget`
- [ ] `camup`
- [ ] `camva`
- [ ] `camzoom`
- [ ] `cla`
- [ ] `clim`
- [ ] `colorbar`
- [ ] `colorcube`
- [ ] `colordef`
- [ ] `colormap`
- [ ] `Colormap Editor`
- [ ] `colororder`
- [ ] `contrast`
- [ ] `cool`
- [ ] `copper`
- [ ] `datatip`
- [ ] `datetick`
- [ ] `diffuse`
- [ ] `flag`
- [ ] `fontname`
- [ ] `fontsize`
- [ ] `ginput`
- [ ] `gray`
- [ ] `graymon`
- [ ] `gtext`
- [ ] `hot`
- [ ] `hsv`
- [ ] `jet`
- [ ] `light`
- [ ] `lightangle`
- [ ] `lighting`
- [ ] `line`
- [ ] `lines`
- [ ] `makehgtform`
- [ ] `material`
- [ ] `nexttile`
- [ ] `num2ruler`
- [ ] `orderedcolors`
- [ ] `parula`
- [ ] `pbaspect`
- [ ] `pink`
- [ ] `plotyy`
- [ ] `prism`
- [ ] `rectangle`
- [ ] `rgbplot`
- [ ] `ruler2num`
- [ ] `shading`
- [ ] `sky`
- [ ] `specular`
- [ ] `spinmap`
- [ ] `spring`
- [ ] `summer`
- [ ] `texlabel`
- [ ] `text`
- [ ] `tightPosition`
- [ ] `tiledlayout`
- [ ] `tilenum`
- [ ] `tilerowcol`
- [ ] `turbo`
- [ ] `view`
- [ ] `viewmtx`
- [ ] `whitebg`
- [ ] `winter`
- [ ] `xline`
- [ ] `xregion`
- [ ] `xscale`
- [ ] `xsecondarylabel`
- [ ] `xtickangle`
- [ ] `xtickformat`
- [ ] `xticklabels`
- [ ] `xticks`
- [ ] `yline`
- [ ] `yregion`
- [ ] `yscale`
- [ ] `ysecondarylabel`
- [ ] `ytickangle`
- [ ] `ytickformat`
- [ ] `yticklabels`
- [ ] `yticks`
- [ ] `yyaxis`
- [ ] `zlabel`
- [ ] `zlim`
- [ ] `zscale`
- [ ] `zsecondarylabel`
- [ ] `ztickangle`
- [ ] `ztickformat`
- [ ] `zticklabels`
- [ ] `zticks`

Implemented: `axis`, `box`, `clf`, `daspect`, `figure`, `grid`, `hold`, `hsv2rgb`, `legend`, `rgb2hsv`, `sgtitle`, `subplot`, `subtitle`, `title`, `xlabel`, `xlim`, `ylabel`, `ylim`

## MATLAB > Graphics > Graphics Objects

Implemented: 3. To implement: 27. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `allchild`
- [ ] `ancestor`
- [ ] `copyobj`
- [ ] `delete`
- [ ] `dragrect`
- [ ] `findall`
- [ ] `findfigs`
- [ ] `findobj`
- [ ] `gcbf`
- [ ] `gcbo`
- [ ] `gco`
- [ ] `gobjects`
- [ ] `groot`
- [ ] `hggroup`
- [ ] `hgtransform`
- [ ] `ishandle`
- [ ] `ishghandle`
- [ ] `matlab.graphics.Graphics`
- [ ] `matlab.graphics.GraphicsPlaceholder`
- [ ] `newplot`
- [ ] `Property Inspector`
- [ ] `rbbox`
- [ ] `refresh`
- [ ] `reset`
- [ ] `shg`
- [ ] `uicontextmenu`
- [ ] `uimenu`

Implemented: `gca`, `gcf`, `ishold`

## MATLAB > Graphics > Graphics Performance

Implemented: 0. To implement: 2. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `opengl`
- [ ] `rendererinfo`

## MATLAB > Graphics > Images

Implemented: 0. To implement: 18. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `cmap2gray`
- [ ] `cmpermute`
- [ ] `cmunique`
- [ ] `dither`
- [ ] `getrangefromclass`
- [ ] `im2double`
- [ ] `im2gray`
- [ ] `im2java`
- [ ] `image`
- [ ] `imagesc`
- [ ] `imapprox`
- [ ] `imformats`
- [ ] `imresize`
- [ ] `imshow`
- [ ] `imtile`
- [ ] `ind2rgb`
- [ ] `rgb2gray`
- [ ] `rgb2ind`

## MATLAB > Graphics > Printing and Saving

Implemented: 0. To implement: 13. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `copygraphics`
- [ ] `exportapp`
- [ ] `exportgraphics`
- [ ] `hgexport`
- [ ] `hgload`
- [ ] `hgsave`
- [ ] `openfig`
- [ ] `orient`
- [ ] `pagesetupdlg`
- [ ] `print`
- [ ] `printopt`
- [ ] `saveas`
- [ ] `savefig`

## MATLAB > Language Fundamentals > Data Types

Implemented: 179. To implement: 219. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addcats`
- [ ] `addevent`
- [ ] `addprop`
- [ ] `addsample`
- [ ] `addsampletocollection`
- [ ] `addtodate`
- [ ] `addts`
- [ ] `addvars`
- [ ] `allfinite`
- [ ] `alphanumericBoundary`
- [ ] `alphanumericsPattern`
- [ ] `anymissing`
- [ ] `anynan`
- [ ] `array2timetable`
- [ ] `asFewOfPattern`
- [ ] `asManyOfPattern`
- [ ] `base2dec`
- [ ] `bin2dec`
- [ ] `caldiff`
- [ ] `calendar`
- [ ] `calendarDuration`
- [ ] `caseInsensitivePattern`
- [ ] `caseSensitivePattern`
- [ ] `categorical`
- [ ] `categories`
- [ ] `cell2mat`
- [ ] `cell2struct`
- [ ] `celldisp`
- [ ] `cellplot`
- [ ] `characterListPattern`
- [ ] `clock`
- [ ] `configureDictionary`
- [ ] `containers.Map`
- [ ] `containsrange`
- [ ] `convertContainedStringsToChars`
- [ ] `convertTo`
- [ ] `convertvars`
- [ ] `countcats`
- [ ] `date`
- [ ] `datestr`
- [ ] `datevec`
- [ ] `dec2base`
- [ ] `dec2bin`
- [ ] `dec2hex`
- [ ] `delevent`
- [ ] `delsample`
- [ ] `delsamplefromcollection`
- [ ] `detectImportOptions`
- [ ] `dictionary`
- [ ] `digitBoundary`
- [ ] `digitsPattern`
- [ ] `entries`
- [ ] `eomdate`
- [ ] `eomday`
- [ ] `eraseBetween`
- [ ] `etime`
- [ ] `eventfilter`
- [ ] `eventtable`
- [ ] `exceltime`
- [ ] `extract`
- [ ] `extractevents`
- [ ] `findEvent`
- [ ] `findstr`
- [ ] `function_handle`
- [ ] `functions`
- [ ] `getabstime`
- [ ] `getdatasamples`
- [ ] `getdatasamplesize`
- [ ] `getfield`
- [ ] `getinterpmethod`
- [ ] `getqualitydesc`
- [ ] `getsamples`
- [ ] `getsampleusingtime`
- [ ] `gettimeseriesnames`
- [ ] `getTimeStr`
- [ ] `gettsafteratevent`
- [ ] `gettsafterevent`
- [ ] `gettsatevent`
- [ ] `gettsbeforeatevent`
- [ ] `gettsbeforeevent`
- [ ] `gettsbetweenevents`
- [ ] `getvaropts`
- [ ] `hex2dec`
- [ ] `hex2num`
- [ ] `hms`
- [ ] `idealfilter`
- [ ] `inner2outer`
- [ ] `innerjoin`
- [ ] `insert`
- [ ] `isbetween`
- [ ] `iscategorical`
- [ ] `iscategory`
- [ ] `isConfigured`
- [ ] `isdst`
- [ ] `isenum`
- [ ] `isgraphics`
- [ ] `isjava`
- [ ] `isKey`
- [ ] `isordinal`
- [ ] `isprotected`
- [ ] `isregular`
- [ ] `issparse`
- [ ] `isstr`
- [ ] `isStringScalar`
- [ ] `istabular`
- [ ] `istimetable`
- [ ] `isundefined`
- [ ] `isUnderlyingType`
- [ ] `isweekend`
- [ ] `juliandate`
- [ ] `keyHash`
- [ ] `keyMatch`
- [ ] `keys`
- [ ] `lag`
- [ ] `leapseconds`
- [ ] `letterBoundary`
- [ ] `lineBoundary`
- [ ] `localfunctions`
- [ ] `lookAheadBoundary`
- [ ] `lookBehindBoundary`
- [ ] `lookup`
- [ ] `lweekdate`
- [ ] `m2xdate`
- [ ] `maskedPattern`
- [ ] `mat2cell`
- [ ] `matches`
- [ ] `matlab.datetime.compatibility.convertDatenum`
- [ ] `mergecats`
- [ ] `mergevars`
- [ ] `months`
- [ ] `movevars`
- [ ] `namedPattern`
- [ ] `native2unicode`
- [ ] `num2hex`
- [ ] `numEntries`
- [ ] `nweekdate`
- [ ] `optionalPattern`
- [ ] `orderfields`
- [ ] `outerjoin`
- [ ] `overlapsrange`
- [ ] `parquetinfo`
- [ ] `parquetread`
- [ ] `parquetwrite`
- [ ] `pattern`
- [ ] `possessivePattern`
- [ ] `preview`
- [ ] `readtimetable`
- [ ] `regexp`
- [ ] `regexpi`
- [ ] `regexpPattern`
- [ ] `regexprep`
- [ ] `regexptranslate`
- [ ] `remove`
- [ ] `removecats`
- [ ] `removets`
- [ ] `renamecats`
- [ ] `reordercats`
- [ ] `replaceBetween`
- [ ] `resample`
- [ ] `retime`
- [ ] `rmprop`
- [ ] `rowfilter`
- [ ] `rowfun`
- [ ] `rows2vars`
- [ ] `setabstime`
- [ ] `setcats`
- [ ] `setfield`
- [ ] `setinterpmethod`
- [ ] `settimeseriesnames`
- [ ] `setuniformtime`
- [ ] `setvaropts`
- [ ] `setvartype`
- [ ] `splitlines`
- [ ] `splitvars`
- [ ] `spreadsheetImportOptions`
- [ ] `sscanf`
- [ ] `stack`
- [ ] `stackedplot`
- [ ] `str2mat`
- [ ] `strmatch`
- [ ] `strread`
- [ ] `strtok`
- [ ] `struct2cell`
- [ ] `structfun`
- [ ] `strvcat`
- [ ] `syncevents`
- [ ] `synchronize`
- [ ] `table2timetable`
- [ ] `textBoundary`
- [ ] `time`
- [ ] `timerange`
- [ ] `timeseries`
- [ ] `timeseries2timetable`
- [ ] `timetable`
- [ ] `timetable2table`
- [ ] `timezones`
- [ ] `tscollection`
- [ ] `tsdata.event`
- [ ] `typecast`
- [ ] `types`
- [ ] `tzoffset`
- [ ] `underlyingType`
- [ ] `unicode2native`
- [ ] `unstack`
- [ ] `values`
- [ ] `varfun`
- [ ] `vartype`
- [ ] `week`
- [ ] `weeknum`
- [ ] `whitespaceBoundary`
- [ ] `whitespacePattern`
- [ ] `whos`
- [ ] `wildcardPattern`
- [ ] `withinrange`
- [ ] `withtol`
- [ ] `writetimetable`
- [ ] `x2mdate`
- [ ] `ymd`
- [ ] `yyyymmdd`

Implemented: `append`, `array2table`, `arrayfun`, `between`, `blanks`, `caldays`, `calmonths`, `calquarters`, `calweeks`, `calyears`, `cast`, `cell`, `cell2table`, `cellfun`, `cellstr`, `char`, `class`, `compose`, `contains`, `convertCharsToStrings`, `convertStringsToChars`, `count`, `datenum`, `dateshift`, `datetime`, `day`, `days`, `deal`, `deblank`, `detrend`, `discretize`, `double`, `duration`, `endsWith`, `eps`, `erase`, `extractAfter`, `extractBefore`, `extractBetween`, `feval`, `fieldnames`, `fillmissing`, `filter`, `findgroups`, `flintmax`, `func2str`, `groupcounts`, `groupfilter`, `groupsummary`, `grouptransform`, `height`, `hour`, `hours`, `Inf`, `insertAfter`, `insertBefore`, `int16`, `int2str`, `int32`, `int64`, `int8`, `intersect`, `intmax`, `intmin`, `iqr`, `isa`, `iscalendarduration`, `iscell`, `iscellstr`, `ischar`, `isdatetime`, `isduration`, `isfield`, `isfinite`, `isfloat`, `isinf`, `isinteger`, `isletter`, `islogical`, `ismember`, `ismissing`, `isnan`, `isnat`, `isnumeric`, `isobject`, `isreal`, `isspace`, `isstring`, `isstrprop`, `isstruct`, `istable`, `join`, `lettersPattern`, `lower`, `mat2str`, `max`, `mean`, `median`, `milliseconds`, `min`, `minute`, `minutes`, `month`, `NaN`, `NaT`, `newline`, `now`, `num2cell`, `num2str`, `pad`, `pivot`, `plot`, `plus`, `posixtime`, `quarter`, `readtable`, `realmax`, `realmin`, `removevars`, `renamevars`, `replace`, `reverse`, `rmfield`, `rmmissing`, `second`, `seconds`, `setdiff`, `setxor`, `single`, `split`, `splitapply`, `sprintf`, `standardizeMissing`, `startsWith`, `std`, `str2double`, `str2func`, `str2num`, `strcat`, `strcmp`, `strcmpi`, `strfind`, `string`, `strings`, `strip`, `strjoin`, `strjust`, `strlength`, `strncmp`, `strncmpi`, `strrep`, `strsplit`, `strtrim`, `struct`, `struct2table`, `sum`, `summary`, `symvar`, `table`, `table2array`, `table2cell`, `table2struct`, `timeofday`, `today`, `topkrows`, `uint16`, `uint32`, `uint64`, `uint8`, `union`, `unique`, `upper`, `validateattributes`, `var`, `weekday`, `width`, `writetable`, `year`, `years`

## MATLAB > Language Fundamentals > Entering Commands

Implemented: 2. To implement: 6. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `commandhistory`
- [ ] `commandwindow`
- [ ] `diary`
- [ ] `home`
- [ ] `iskeyword`
- [ ] `more`

Language keywords to check: `ans`

Implemented: `clc`, `format`

## MATLAB > Language Fundamentals > Matrices and Arrays

Implemented: 54. To implement: 3. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `combinations`
- [ ] `inline`
- [ ] `vectorize`

Language keywords to check: `end`

Implemented: `blkdiag`, `cat`, `circshift`, `colon`, `ctranspose`, `diag`, `eye`, `false`, `flip`, `flipdim`, `fliplr`, `flipud`, `freqspace`, `head`, `horzcat`, `ind2sub`, `ipermute`, `iscolumn`, `isempty`, `ismatrix`, `isrow`, `isscalar`, `issorted`, `issortedrows`, `isuniform`, `isvector`, `length`, `linspace`, `logspace`, `meshgrid`, `ndgrid`, `ndims`, `numel`, `ones`, `paddata`, `permute`, `rand`, `repelem`, `repmat`, `reshape`, `resize`, `rot90`, `shiftdim`, `size`, `sort`, `sortrows`, `squeeze`, `sub2ind`, `tail`, `transpose`, `trimdata`, `true`, `vertcat`, `zeros`

## MATLAB > Language Fundamentals > Operators and Elementary Operations

Implemented: 36. To implement: 26. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `&amp;`
- [ ] `&gt;`
- [ ] `&gt;=`
- [ ] `&lt;`
- [ ] `&lt;=`
- [ ] `'`
- [ ] `*`
- [ ] `+`
- [ ] `-`
- [ ] `.'`
- [ ] `.*`
- [ ] `./`
- [ ] `.\`
- [ ] `.^`
- [ ] `/`
- [ ] `==`
- [ ] `\`
- [ ] `^`
- [ ] `ismembertol`
- [ ] `Short-Circuit &amp;&amp;`
- [ ] `Short-Circuit ||`
- [ ] `swapbytes`
- [ ] `uniquetol`
- [ ] `|`
- [ ] `~`
- [ ] `~=`

Implemented: `all`, `any`, `bitand`, `bitcmp`, `bitget`, `bitor`, `bitset`, `bitshift`, `bitxor`, `bsxfun`, `ceil`, `cumprod`, `cumsum`, `diff`, `find`, `fix`, `floor`, `idivide`, `isequal`, `isequaln`, `isequalwithequalnans`, `logical`, `mod`, `movsum`, `pagectranspose`, `pagemldivide`, `pagemrdivide`, `pagemtimes`, `pagetranspose`, `prod`, `rem`, `round`, `tensorprod`, `uminus`, `uplus`, `xor`

## MATLAB > Mathematics > Computational Geometry

Implemented: 18. To implement: 66. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addboundary`
- [ ] `alphaSpectrum`
- [ ] `alphaTriangulation`
- [ ] `barycentricToCartesian`
- [ ] `baryToCart`
- [ ] `boundaryFacets`
- [ ] `boundaryshape`
- [ ] `boundingbox`
- [ ] `cartesianToBarycentric`
- [ ] `cartToBary`
- [ ] `centroid`
- [ ] `circumcenter`
- [ ] `circumcenters`
- [ ] `convexHull`
- [ ] `criticalAlpha`
- [ ] `DelaunayTri`
- [ ] `edgeAttachments`
- [ ] `edges`
- [ ] `faceNormal`
- [ ] `faceNormals`
- [ ] `featureEdges`
- [ ] `freeBoundary`
- [ ] `holes`
- [ ] `incenter`
- [ ] `incenters`
- [ ] `inOutStatus`
- [ ] `inShape`
- [ ] `isConnected`
- [ ] `isEdge`
- [ ] `ishole`
- [ ] `isInterior`
- [ ] `isinterior`
- [ ] `issimplified`
- [ ] `nearestNeighbor`
- [ ] `nearestvertex`
- [ ] `numboundaries`
- [ ] `numRegions`
- [ ] `numsides`
- [ ] `overlaps`
- [ ] `patch`
- [ ] `perimeter`
- [ ] `pointLocation`
- [ ] `polybuffer`
- [ ] `regions`
- [ ] `rmboundary`
- [ ] `rmholes`
- [ ] `rmslivers`
- [ ] `rotate`
- [ ] `scale`
- [ ] `sortboundaries`
- [ ] `sortregions`
- [ ] `stlread`
- [ ] `stlwrite`
- [ ] `subtract`
- [ ] `surfaceArea`
- [ ] `tetramesh`
- [ ] `translate`
- [ ] `trimesh`
- [ ] `triplot`
- [ ] `TriRep`
- [ ] `trisurf`
- [ ] `turningdist`
- [ ] `vertexAttachments`
- [ ] `vertexNormal`
- [ ] `volume`
- [ ] `voronoiDiagram`

Implemented: `alphaShape`, `area`, `boundary`, `convhull`, `convhulln`, `delaunay`, `delaunayn`, `delaunayTriangulation`, `dsearchn`, `inpolygon`, `nsidedpoly`, `polyarea`, `polyshape`, `rectint`, `triangulation`, `tsearchn`, `voronoi`, `voronoin`

## MATLAB > Mathematics > Elementary Math

Implemented: 126. To implement: 4. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `cospi`
- [ ] `matchpairs`
- [ ] `residue`
- [ ] `sinpi`

Implemented: `abs`, `acos`, `acosd`, `acosh`, `acot`, `acotd`, `acoth`, `acsc`, `acscd`, `acsch`, `airy`, `angle`, `asec`, `asecd`, `asech`, `asin`, `asind`, `asinh`, `atan`, `atan2`, `atan2d`, `atand`, `atanh`, `besselh`, `besseli`, `besselj`, `besselk`, `bessely`, `beta`, `betainc`, `betaincinv`, `betaln`, `cart2pol`, `cart2sph`, `compan`, `complex`, `conj`, `conv`, `cos`, `cosd`, `cosh`, `cot`, `cotd`, `coth`, `cplxpair`, `csc`, `cscd`, `csch`, `deconv`, `deg2rad`, `ellipj`, `ellipke`, `erf`, `erfc`, `erfcinv`, `erfcx`, `erfinv`, `exp`, `expint`, `expm1`, `factor`, `factorial`, `gallery`, `gamma`, `gammainc`, `gammaincinv`, `gammaln`, `gcd`, `hadamard`, `hankel`, `hilb`, `hypot`, `i`, `imag`, `invhilb`, `isprime`, `j`, `lcm`, `legendre`, `log`, `log10`, `log1p`, `log2`, `magic`, `nchoosek`, `nextpow2`, `nthroot`, `pascal`, `perms`, `pi`, `pol2cart`, `poly`, `polyder`, `polyeig`, `polyfit`, `polyint`, `polyval`, `polyvalm`, `pow2`, `primes`, `psi`, `rad2deg`, `rat`, `rats`, `real`, `reallog`, `realpow`, `realsqrt`, `roots`, `rosser`, `sec`, `secd`, `sech`, `sign`, `sin`, `sind`, `sinh`, `sph2cart`, `sqrt`, `tan`, `tand`, `tanh`, `toeplitz`, `unwrap`, `vander`, `wilkinson`

## MATLAB > Mathematics > Fourier Analysis and Filtering

Implemented: 14. To implement: 1. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `ss2tf`

Implemented: `conv2`, `convn`, `fft`, `fft2`, `fftn`, `fftshift`, `fftw`, `filter2`, `ifft`, `ifft2`, `ifftn`, `ifftshift`, `nufft`, `nufftn`

## MATLAB > Mathematics > Graph and Network Algorithms

Implemented: 1. To implement: 53. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addedge`
- [ ] `addnode`
- [ ] `adjacency`
- [ ] `allcycles`
- [ ] `allpaths`
- [ ] `bctree`
- [ ] `bfsearch`
- [ ] `biconncomp`
- [ ] `centrality`
- [ ] `condensation`
- [ ] `conncomp`
- [ ] `cyclebasis`
- [ ] `degree`
- [ ] `dfsearch`
- [ ] `digraph`
- [ ] `distances`
- [ ] `edgecount`
- [ ] `findedge`
- [ ] `findnode`
- [ ] `flipedge`
- [ ] `graph`
- [ ] `hascycles`
- [ ] `highlight`
- [ ] `incidence`
- [ ] `indegree`
- [ ] `inedges`
- [ ] `isdag`
- [ ] `isisomorphic`
- [ ] `ismultigraph`
- [ ] `isomorphism`
- [ ] `labeledge`
- [ ] `labelnode`
- [ ] `laplacian`
- [ ] `layout`
- [ ] `maxflow`
- [ ] `minspantree`
- [ ] `nearest`
- [ ] `neighbors`
- [ ] `numedges`
- [ ] `numnodes`
- [ ] `outdegree`
- [ ] `outedges`
- [ ] `predecessors`
- [ ] `reordernodes`
- [ ] `rmedge`
- [ ] `rmnode`
- [ ] `shortestpath`
- [ ] `shortestpathtree`
- [ ] `subgraph`
- [ ] `successors`
- [ ] `toposort`
- [ ] `transclosure`
- [ ] `transreduction`

Implemented: `simplify`

## MATLAB > Mathematics > Interpolation

Implemented: 15. To implement: 3. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `interp1q`
- [ ] `padecoef`
- [ ] `TriScatteredInterp`

Implemented: `griddata`, `griddatan`, `griddedInterpolant`, `interp1`, `interp2`, `interp3`, `interpft`, `interpn`, `makima`, `mkpp`, `pchip`, `ppval`, `scatteredInterpolant`, `spline`, `unmkpp`

## MATLAB > Mathematics > Linear Algebra

Implemented: 65. To implement: 8. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `decomposition`
- [ ] `expmv`
- [ ] `isIllConditioned`
- [ ] `lsqnonneg`
- [ ] `pageinv`
- [ ] `pagenorm`
- [ ] `svdappend`
- [ ] `svdsketch`

Implemented: `balance`, `bandwidth`, `cdf2rdf`, `chol`, `cholupdate`, `cond`, `condeig`, `condest`, `cross`, `det`, `dot`, `eig`, `eigs`, `expm`, `funm`, `gsvd`, `hess`, `inv`, `isbanded`, `isdiag`, `ishermitian`, `issymmetric`, `istril`, `istriu`, `kron`, `ldl`, `linsolve`, `logm`, `lscov`, `lsqminnorm`, `lu`, `mldivide`, `mpower`, `mrdivide`, `mtimes`, `norm`, `normest`, `null`, `ordeig`, `ordqz`, `ordschur`, `orth`, `pageeig`, `pagesvd`, `pinv`, `planerot`, `qr`, `qrdelete`, `qrinsert`, `qrupdate`, `qz`, `rank`, `rcond`, `rref`, `rsf2csf`, `schur`, `sqrtm`, `subspace`, `svd`, `svds`, `sylvester`, `trace`, `tril`, `triu`, `vecnorm`

## MATLAB > Mathematics > Numerical Integration and Differential Equations

Implemented: 5. To implement: 39. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `bvp4c`
- [ ] `bvp5c`
- [ ] `bvpget`
- [ ] `bvpinit`
- [ ] `bvpset`
- [ ] `bvpxtend`
- [ ] `dblquad`
- [ ] `dde23`
- [ ] `ddeget`
- [ ] `ddensd`
- [ ] `ddesd`
- [ ] `ddeset`
- [ ] `decic`
- [ ] `deval`
- [ ] `integral`
- [ ] `integral2`
- [ ] `integral3`
- [ ] `ode113`
- [ ] `ode15i`
- [ ] `ode15s`
- [ ] `ode23`
- [ ] `ode23s`
- [ ] `ode23t`
- [ ] `ode23tb`
- [ ] `ode45`
- [ ] `ode78`
- [ ] `ode89`
- [ ] `odeget`
- [ ] `odeset`
- [ ] `odextend`
- [ ] `pdepe`
- [ ] `pdeval`
- [ ] `quad`
- [ ] `quad2d`
- [ ] `quadgk`
- [ ] `quadl`
- [ ] `quadv`
- [ ] `solutionFcn`
- [ ] `triplequad`

Implemented: `cumtrapz`, `del2`, `gradient`, `solve`, `trapz`

## MATLAB > Mathematics > Optimization

Implemented: 0. To implement: 5. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `fminbnd`
- [ ] `fminsearch`
- [ ] `fzero`
- [ ] `optimget`
- [ ] `optimset`

## MATLAB > Mathematics > Quantum Computing

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 31.

Out of scope: `ccxGate` (Mathematics > Quantum Computing), `chGate` (Mathematics > Quantum Computing), `cnotGate` (Mathematics > Quantum Computing), `compositeGate` (Mathematics > Quantum Computing), `cr1Gate` (Mathematics > Quantum Computing), `crxGate` (Mathematics > Quantum Computing), `cryGate` (Mathematics > Quantum Computing), `crzGate` (Mathematics > Quantum Computing), `cxGate` (Mathematics > Quantum Computing), `cyGate` (Mathematics > Quantum Computing), `czGate` (Mathematics > Quantum Computing), `evaluateObjective` (Mathematics > Quantum Computing), `hGate` (Mathematics > Quantum Computing), `idGate` (Mathematics > Quantum Computing), `mcxGate` (Mathematics > Quantum Computing), `qftGate` (Mathematics > Quantum Computing), `r1Gate` (Mathematics > Quantum Computing), `rxGate` (Mathematics > Quantum Computing), `rxxGate` (Mathematics > Quantum Computing), `ryGate` (Mathematics > Quantum Computing), `ryyGate` (Mathematics > Quantum Computing), `rzGate` (Mathematics > Quantum Computing), `rzzGate` (Mathematics > Quantum Computing), `sGate` (Mathematics > Quantum Computing), `siGate` (Mathematics > Quantum Computing), `swapGate` (Mathematics > Quantum Computing), `tGate` (Mathematics > Quantum Computing), `tiGate` (Mathematics > Quantum Computing), `xGate` (Mathematics > Quantum Computing), `yGate` (Mathematics > Quantum Computing), `zGate` (Mathematics > Quantum Computing)

## MATLAB > Mathematics > Random Number Generation

Implemented: 4. To implement: 6. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `RandStream`
- [ ] `RandStream.create`
- [ ] `RandStream.getGlobalStream`
- [ ] `RandStream.list`
- [ ] `RandStream.setGlobalStream`
- [ ] `reset (RandStream)`

Implemented: `randi`, `randn`, `randperm`, `rng`

## MATLAB > Mathematics > Sparse Matrices

Implemented: 1. To implement: 45. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `amd`
- [ ] `bicg`
- [ ] `bicgstab`
- [ ] `bicgstabl`
- [ ] `cgs`
- [ ] `colamd`
- [ ] `colperm`
- [ ] `dissect`
- [ ] `dmperm`
- [ ] `equilibrate`
- [ ] `etree`
- [ ] `etreeplot`
- [ ] `full`
- [ ] `gmres`
- [ ] `gplot`
- [ ] `ichol`
- [ ] `ilu`
- [ ] `lsqr`
- [ ] `minres`
- [ ] `nonzeros`
- [ ] `nzmax`
- [ ] `pcg`
- [ ] `qmr`
- [ ] `spalloc`
- [ ] `sparse`
- [ ] `spaugment`
- [ ] `spconvert`
- [ ] `spdiags`
- [ ] `speye`
- [ ] `spfun`
- [ ] `spones`
- [ ] `spparms`
- [ ] `sprand`
- [ ] `sprandn`
- [ ] `sprandsym`
- [ ] `sprank`
- [ ] `spy`
- [ ] `symamd`
- [ ] `symbfact`
- [ ] `symmlq`
- [ ] `symrcm`
- [ ] `tfqmr`
- [ ] `treelayout`
- [ ] `treeplot`
- [ ] `unmesh`

Implemented: `nnz`

## MATLAB > Programming > Classes

Implemented: 11. To implement: 25. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addlistener`
- [ ] `builtin`
- [ ] `clone`
- [ ] `details`
- [ ] `empty`
- [ ] `enumeration`
- [ ] `event.hasListener`
- [ ] `events`
- [ ] `import`
- [ ] `isDone`
- [ ] `isLocked`
- [ ] `listener`
- [ ] `listLength`
- [ ] `loadobj`
- [ ] `meta.abstractDetails`
- [ ] `meta.class.fromName`
- [ ] `meta.package.fromName`
- [ ] `meta.package.getAllPackages`
- [ ] `notify`
- [ ] `release`
- [ ] `saveobj`
- [ ] `setup`
- [ ] `step`
- [ ] `superclasses`
- [ ] `sysobjupdate`

Language keywords to check: `classdef`

Implemented: `eq`, `ismethod`, `isprop`, `metaclass`, `methods`, `numArgumentsFromSubscript`, `properties`, `subsasgn`, `subsindex`, `subsref`, `substruct`

## MATLAB > Programming > Files and Folders

Implemented: 5. To implement: 40. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `cd`
- [ ] `copyfile`
- [ ] `dir`
- [ ] `fileattrib`
- [ ] `filebrowser`
- [ ] `filemarker`
- [ ] `fileparts`
- [ ] `filesep`
- [ ] `fullfile`
- [ ] `genpath`
- [ ] `gunzip`
- [ ] `gzip`
- [ ] `isdir`
- [ ] `isfile`
- [ ] `isfolder`
- [ ] `ls`
- [ ] `matlabdrive`
- [ ] `matlabroot`
- [ ] `mkdir`
- [ ] `movefile`
- [ ] `path2rc`
- [ ] `pathsep`
- [ ] `pathtool`
- [ ] `recycle`
- [ ] `rehash`
- [ ] `restoredefaultpath`
- [ ] `rmdir`
- [ ] `savepath`
- [ ] `tar`
- [ ] `tempdir`
- [ ] `tempname`
- [ ] `toolboxdir`
- [ ] `untar`
- [ ] `unzip`
- [ ] `userpath`
- [ ] `visdiff`
- [ ] `what`
- [ ] `which`
- [ ] `winopen`
- [ ] `zip`

Implemented: `addpath`, `exist`, `path`, `pwd`, `rmpath`

## MATLAB > Programming > Functions

Implemented: 6. To implement: 56. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addOptional`
- [ ] `addParameter`
- [ ] `addParamValue`
- [ ] `addRequired`
- [ ] `assignin`
- [ ] `genvarname`
- [ ] `inputParser`
- [ ] `isvarname`
- [ ] `lasterr`
- [ ] `lasterror`
- [ ] `lastwarn`
- [ ] `matlab.lang.makeUniqueStrings`
- [ ] `matlab.lang.makeValidName`
- [ ] `mfilename`
- [ ] `mislocked`
- [ ] `mlock`
- [ ] `munlock`
- [ ] `mustBeA`
- [ ] `mustBeFile`
- [ ] `mustBeFinite`
- [ ] `mustBeFloat`
- [ ] `mustBeFolder`
- [ ] `mustBeGreaterThan`
- [ ] `mustBeGreaterThanOrEqual`
- [ ] `mustBeInRange`
- [ ] `mustBeInteger`
- [ ] `mustBeLessThan`
- [ ] `mustBeLessThanOrEqual`
- [ ] `mustBeMember`
- [ ] `mustBeNegative`
- [ ] `mustBeNonempty`
- [ ] `mustBeNonmissing`
- [ ] `mustBeNonNan`
- [ ] `mustBeNonnegative`
- [ ] `mustBeNonpositive`
- [ ] `mustBeNonsparse`
- [ ] `mustBeNonzero`
- [ ] `mustBeNonzeroLengthText`
- [ ] `mustBeNumeric`
- [ ] `mustBeNumericOrLogical`
- [ ] `mustBePositive`
- [ ] `mustBeReal`
- [ ] `mustBeScalarOrEmpty`
- [ ] `mustBeSparse`
- [ ] `mustBeText`
- [ ] `mustBeTextScalar`
- [ ] `mustBeUnderlyingType`
- [ ] `mustBeValidVariableName`
- [ ] `mustBeVector`
- [ ] `namedargs2cell`
- [ ] `namelengthmax`
- [ ] `nargoutchk`
- [ ] `onCleanup`
- [ ] `parse`
- [ ] `validatecolor`
- [ ] `validatestring`

Language keywords to check: `arguments`, `global`, `nargin`, `nargout`, `persistent`, `varargin`, `varargout`

Implemented: `assert`, `error`, `inputname`, `nargchk`, `narginchk`, `warning`

## MATLAB > Programming > Live Scripts and Functions

Implemented: 0. To implement: 1. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `export`

## MATLAB > Programming > Programming Utilities

Implemented: 1. To implement: 28. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addCause`
- [ ] `addCorrection`
- [ ] `canUseGPU`
- [ ] `canUseParallelPool`
- [ ] `eval`
- [ ] `evalc`
- [ ] `evalin`
- [ ] `getReport`
- [ ] `isvalid`
- [ ] `matlab.lang.correction.AppendArgumentsCorrection`
- [ ] `matlab.lang.correction.ConvertToFunctionNotationCorrection`
- [ ] `matlab.lang.correction.ReplaceIdentifierCorrection`
- [ ] `MException`
- [ ] `MException.last`
- [ ] `parallel.pool.Constant`
- [ ] `parfevalOnAll`
- [ ] `rethrow`
- [ ] `run`
- [ ] `start`
- [ ] `startat`
- [ ] `stop`
- [ ] `throw`
- [ ] `throwAsCaller`
- [ ] `timer`
- [ ] `timerfind`
- [ ] `timerfindall`
- [ ] `try, catch`
- [ ] `wait`

Implemented: `maxNumCompThreads`

## MATLAB > Programming > Scripts

Implemented: 0. To implement: 6. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `edit`
- [ ] `grabcode`
- [ ] `input`
- [ ] `notebook`
- [ ] `publish`
- [ ] `snapnow`

## MATLAB > Software Development Tools > Background Processing

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 13.

Out of scope: `afterAll` (Software Development Tools > Background Processing), `afterEach` (Software Development Tools > Background Processing), `backgroundPool` (Software Development Tools > Background Processing), `cancel` (Software Development Tools > Background Processing), `cancelAll` (Software Development Tools > Background Processing), `fetchNext` (Software Development Tools > Background Processing), `fetchOutputs` (Software Development Tools > Background Processing), `Future` (Software Development Tools > Background Processing), `parallel.pool.DataQueue` (Software Development Tools > Background Processing), `parallel.pool.PollableDataQueue` (Software Development Tools > Background Processing), `parfeval` (Software Development Tools > Background Processing), `poll` (Software Development Tools > Background Processing), `send` (Software Development Tools > Background Processing)

## MATLAB > Software Development Tools > Build Automation

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 2.

Out of scope: `buildplan` (Software Development Tools > Build Automation), `buildtool` (Software Development Tools > Build Automation)

## MATLAB > Software Development Tools > Debugging and Analysis

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 21.

Out of scope: `analyzeCodeCompatibility` (Software Development Tools > Debugging and Analysis), `checkcode` (Software Development Tools > Debugging and Analysis), `CodeCompatibilityAnalysis` (Software Development Tools > Debugging and Analysis), `codeCompatibilityReport` (Software Development Tools > Debugging and Analysis), `codeIssues` (Software Development Tools > Debugging and Analysis), `dbclear` (Software Development Tools > Debugging and Analysis), `dbcont` (Software Development Tools > Debugging and Analysis), `dbdown` (Software Development Tools > Debugging and Analysis), `dbquit` (Software Development Tools > Debugging and Analysis), `dbstack` (Software Development Tools > Debugging and Analysis), `dbstatus` (Software Development Tools > Debugging and Analysis), `dbstep` (Software Development Tools > Debugging and Analysis), `dbstop` (Software Development Tools > Debugging and Analysis), `dbtype` (Software Development Tools > Debugging and Analysis), `dbup` (Software Development Tools > Debugging and Analysis), `echo` (Software Development Tools > Debugging and Analysis), `keyboard` (Software Development Tools > Debugging and Analysis), `matlab.codeanalysis.refreshConfiguration` (Software Development Tools > Debugging and Analysis), `matlab.codeanalysis.validateConfiguration` (Software Development Tools > Debugging and Analysis), `mlint` (Software Development Tools > Debugging and Analysis), `mlintrpt` (Software Development Tools > Debugging and Analysis)

## MATLAB > Software Development Tools > Performance and Memory

Implemented: 2. To implement: 12. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `bench`
- [ ] `clearAllMemoizedCaches`
- [ ] `clearCache`
- [ ] `cputime`
- [ ] `inmem`
- [ ] `memoize`
- [ ] `MemoizedFunction`
- [ ] `memory`
- [ ] `profile`
- [ ] `profsave`
- [ ] `stats`
- [ ] `timeit`

Implemented: `tic`, `toc`

## MATLAB > Software Development Tools > Projects

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 40.

Out of scope: `addFile` (Software Development Tools > Projects), `addFolderIncludingChildFiles` (Software Development Tools > Projects), `addLabel` (Software Development Tools > Projects), `addPath` (Software Development Tools > Projects), `addReference` (Software Development Tools > Projects), `addShortcut` (Software Development Tools > Projects), `addShutdownFile` (Software Development Tools > Projects), `addStartupFile` (Software Development Tools > Projects), `createCategory` (Software Development Tools > Projects), `createLabel` (Software Development Tools > Projects), `currentProject` (Software Development Tools > Projects), `findCategory` (Software Development Tools > Projects), `findFile` (Software Development Tools > Projects), `findLabel` (Software Development Tools > Projects), `isLoaded` (Software Development Tools > Projects), `listAllProjectReferences` (Software Development Tools > Projects), `listImpactedFiles` (Software Development Tools > Projects), `listModifiedFiles` (Software Development Tools > Projects), `listRequiredFiles` (Software Development Tools > Projects), `matlab.project.convertDefinitionFiles` (Software Development Tools > Projects), `matlab.project.createProject` (Software Development Tools > Projects), `matlab.project.deleteProject` (Software Development Tools > Projects), `matlab.project.extractProject` (Software Development Tools > Projects), `matlab.project.isFileInProject` (Software Development Tools > Projects), `matlab.project.isUnderProjectRoot` (Software Development Tools > Projects), `matlab.project.loadProject` (Software Development Tools > Projects), `matlab.project.rootProject` (Software Development Tools > Projects), `openProject` (Software Development Tools > Projects), `refreshSourceControl` (Software Development Tools > Projects), `reload` (Software Development Tools > Projects), `removeCategory` (Software Development Tools > Projects), `removeFile` (Software Development Tools > Projects), `removeLabel` (Software Development Tools > Projects), `removePath` (Software Development Tools > Projects), `removeReference` (Software Development Tools > Projects), `removeShortcut` (Software Development Tools > Projects), `removeShutdownFile` (Software Development Tools > Projects), `removeStartupFile` (Software Development Tools > Projects), `runChecks` (Software Development Tools > Projects), `updateDependencies` (Software Development Tools > Projects)

## MATLAB > Software Development Tools > Source Control Integration

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 19.

Out of scope: `checkin` (Software Development Tools > Source Control Integration), `checkout` (Software Development Tools > Source Control Integration), `cmopts` (Software Development Tools > Source Control Integration), `commit` (Software Development Tools > Source Control Integration), `createBranch` (Software Development Tools > Source Control Integration), `customverctrl` (Software Development Tools > Source Control Integration), `deleteBranch` (Software Development Tools > Source Control Integration), `fetch` (Software Development Tools > Source Control Integration), `gitclone` (Software Development Tools > Source Control Integration), `gitinit` (Software Development Tools > Source Control Integration), `gitrepo` (Software Development Tools > Source Control Integration), `merge` (Software Development Tools > Source Control Integration), `pull` (Software Development Tools > Source Control Integration), `push` (Software Development Tools > Source Control Integration), `rm` (Software Development Tools > Source Control Integration), `status` (Software Development Tools > Source Control Integration), `switchBranch` (Software Development Tools > Source Control Integration), `undocheckout` (Software Development Tools > Source Control Integration), `verctrl` (Software Development Tools > Source Control Integration)

## MATLAB > Software Development Tools > Testing Frameworks

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 17.

Out of scope: `choose` (Software Development Tools > Testing Frameworks), `chooseContextMenu` (Software Development Tools > Testing Frameworks), `dismissAlertDialog` (Software Development Tools > Testing Frameworks), `drag` (Software Development Tools > Testing Frameworks), `functiontests` (Software Development Tools > Testing Frameworks), `hover` (Software Development Tools > Testing Frameworks), `matlab.uitest.TestCase.forInteractiveUse` (Software Development Tools > Testing Frameworks), `matlab.uitest.unlock` (Software Development Tools > Testing Frameworks), `press` (Software Development Tools > Testing Frameworks), `run (TestCase)` (Software Development Tools > Testing Frameworks), `run (TestRunner)` (Software Development Tools > Testing Frameworks), `run (TestSuite)` (Software Development Tools > Testing Frameworks), `runInParallel` (Software Development Tools > Testing Frameworks), `runperf` (Software Development Tools > Testing Frameworks), `runtests` (Software Development Tools > Testing Frameworks), `testrunner` (Software Development Tools > Testing Frameworks), `testsuite` (Software Development Tools > Testing Frameworks)

## MATLAB > Software Development Tools > Toolbox Distribution

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 21.

Out of scope: `builddocsearchdb` (Software Development Tools > Toolbox Distribution), `FactoryGroup` (Software Development Tools > Toolbox Distribution), `FactorySetting` (Software Development Tools > Toolbox Distribution), `matlab.addons.toolbox.installedToolboxes` (Software Development Tools > Toolbox Distribution), `matlab.addons.toolbox.installToolbox` (Software Development Tools > Toolbox Distribution), `matlab.addons.toolbox.packageToolbox` (Software Development Tools > Toolbox Distribution), `matlab.addons.toolbox.ToolboxOptions` (Software Development Tools > Toolbox Distribution), `matlab.addons.toolbox.toolboxVersion` (Software Development Tools > Toolbox Distribution), `matlab.addons.toolbox.uninstallToolbox` (Software Development Tools > Toolbox Distribution), `matlab.codetools.requiredFilesAndProducts` (Software Development Tools > Toolbox Distribution), `matlab.settings.FactoryGroup.createToolboxGroup` (Software Development Tools > Toolbox Distribution), `matlab.settings.loadSettingsCompatibilityResults` (Software Development Tools > Toolbox Distribution), `matlab.settings.reloadFactoryFile` (Software Development Tools > Toolbox Distribution), `matlab.settings.SettingsFileUpgrader` (Software Development Tools > Toolbox Distribution), `OperationResult` (Software Development Tools > Toolbox Distribution), `patchdemoxmlfile` (Software Development Tools > Toolbox Distribution), `pcode` (Software Development Tools > Toolbox Distribution), `ReleaseCompatibilityException` (Software Development Tools > Toolbox Distribution), `ReleaseCompatibilityResults` (Software Development Tools > Toolbox Distribution), `validateFunctionSignaturesJSON` (Software Development Tools > Toolbox Distribution), `VersionResults` (Software Development Tools > Toolbox Distribution)

## Not in the function reference > matlab/audiovideo

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 4.

Out of scope: `audiouniquename` (audio/video devices or screen capture), `avgate` (audio/video devices or screen capture), `avifinfo` (audio/video devices or screen capture), `prefspanel` (audio/video devices or screen capture)

## Not in the function reference > matlab/datatypes

Implemented: 0. To implement: 5. In a branch: 0. Out of scope: 3.

To implement:

- [ ] `inferiorto`
- [ ] `superiorfloat`
- [ ] `superiorto`
- [ ] `tabular`
- [ ] `ts2timetable`

Out of scope: `DefaultObject` (internal helper, not a user function), `internalArrayUtil` (internal helper, not a user function), `opaque` (Java/.NET/COM/C/Perl interop or MEX)

## Not in the function reference > matlab/elmat

Implemented: 2. To implement: 0. In a branch: 0. Out of scope: 0.

Implemented: `inf`, `nan`

## Not in the function reference > matlab/funfun

Implemented: 0. To implement: 12. In a branch: 0. Out of scope: 47.

To implement:

- [ ] `fcnchk`
- [ ] `inlineeval`
- [ ] `numjac`
- [ ] `ode`
- [ ] `odeEvent`
- [ ] `odeJacobian`
- [ ] `odeMassMatrix`
- [ ] `odephas2`
- [ ] `odephas3`
- [ ] `odeplot`
- [ ] `odeprint`
- [ ] `pde`

Out of scope: `amp1dae` (MATLAB example file), `ballode` (MATLAB example file), `batonode` (MATLAB example file), `brussode` (MATLAB example file), `burgersode` (MATLAB example file), `ddex1` (MATLAB example file), `ddex1de` (MATLAB example file), `ddex1delays` (MATLAB example file), `ddex1hist` (MATLAB example file), `ddex2` (MATLAB example file), `ddex3` (MATLAB example file), `ddex4` (MATLAB example file), `ddex5` (MATLAB example file), `emdenbvp` (MATLAB example file), `fem1ode` (MATLAB example file), `fem2ode` (MATLAB example file), `fsbvp` (MATLAB example file), `hb1dae` (MATLAB example file), `hb1ode` (MATLAB example file), `iburgersode` (MATLAB example file), `ihb1dae` (MATLAB example file), `kneeode` (MATLAB example file), `lorenz` (MATLAB example file), `lotka` (MATLAB example file), `mat4bvp` (MATLAB example file), `odeexamples` (MATLAB example file), `orbitode` (MATLAB example file), `pdex1` (MATLAB example file), `pdex1bc` (MATLAB example file), `pdex1ic` (MATLAB example file), `pdex1pde` (MATLAB example file), `pdex2` (MATLAB example file), `pdex3` (MATLAB example file), `pdex4` (MATLAB example file), `pdex5` (MATLAB example file), `rcbvp` (MATLAB example file), `rigidode` (MATLAB example file), `shockbvp` (MATLAB example file), `threebvp` (MATLAB example file), `twobc` (MATLAB example file), `twobvp` (MATLAB example file), `twoode` (MATLAB example file), `vanderpoldemo` (MATLAB example file), `vdp1` (MATLAB example file), `vdp1000` (MATLAB example file), `vdpode` (MATLAB example file), `weissinger` (MATLAB example file)

## Not in the function reference > matlab/general

Implemented: 0. To implement: 2. In a branch: 0. Out of scope: 15.

To implement:

- [ ] `matlab`
- [ ] `matlabpath`

Out of scope: `callgraphviz` (desktop, IDE, GUI, printing or interactive tool), `depfunprophelper` (desktop, IDE, GUI, printing or interactive tool), `desktop` (desktop, IDE, GUI, printing or interactive tool), `findfile` (desktop, IDE, GUI, printing or interactive tool), `finfo` (desktop, IDE, GUI, printing or interactive tool), `getmcruserdata` (desktop, IDE, GUI, printing or interactive tool), `isdeployed` (desktop, IDE, GUI, printing or interactive tool), `ismcc` (desktop, IDE, GUI, printing or interactive tool), `java` (Java/.NET/COM/C/Perl interop or MEX), `logo` (desktop, IDE, GUI, printing or interactive tool), `membrane` (desktop, IDE, GUI, printing or interactive tool), `mexdebug` (Java/.NET/COM/C/Perl interop or MEX), `preferences` (desktop, IDE, GUI, printing or interactive tool), `setmcruserdata` (desktop, IDE, GUI, printing or interactive tool), `syntax` (desktop, IDE, GUI, printing or interactive tool)

## Not in the function reference > matlab/graph2d

Implemented: 0. To implement: 1. In a branch: 0. Out of scope: 33.

To implement:

- [ ] `getcolumn`

Out of scope: `arrowline` (internal helper, not a user function), `axischild` (internal helper, not a user function), `axisobj` (internal helper, not a user function), `axistext` (internal helper, not a user function), `basicfitdatastat` (internal helper, not a user function), `doclick` (internal helper, not a user function), `dokeypress` (internal helper, not a user function), `domymenu` (internal helper, not a user function), `doresize` (internal helper, not a user function), `editline` (internal helper, not a user function), `editrect` (internal helper, not a user function), `enddrag` (internal helper, not a user function), `fighandle` (internal helper, not a user function), `figobj` (internal helper, not a user function), `figtoolset` (internal helper, not a user function), `getobj` (internal helper, not a user function), `getorcreateobj` (internal helper, not a user function), `getscribecontextmenu` (internal helper, not a user function), `getscribeobjectdata` (internal helper, not a user function), `hgbin` (internal helper, not a user function), `jpropeditutils` (internal helper, not a user function), `middrag` (internal helper, not a user function), `moveaxis` (internal helper, not a user function), `prepdrag` (internal helper, not a user function), `putdowntext` (internal helper, not a user function), `scribeclearmode` (internal helper, not a user function), `scribeeventhandler` (internal helper, not a user function), `scribehandle` (internal helper, not a user function), `scribehgobj` (internal helper, not a user function), `scriberestoresavefcns` (internal helper, not a user function), `scribetextdlg` (internal helper, not a user function), `setscribecontextmenu` (internal helper, not a user function), `setscribeobjectdata` (internal helper, not a user function)

## Not in the function reference > matlab/graph3d

Implemented: 0. To implement: 3. In a branch: 0. Out of scope: 3.

To implement:

- [ ] `camrotate`
- [ ] `caxis`
- [ ] `objbounds`

Out of scope: `cameramenu` (desktop, IDE, GUI, printing or interactive tool), `cmapeditor` (desktop, IDE, GUI, printing or interactive tool), `colormapeditor` (desktop, IDE, GUI, printing or interactive tool)

## Not in the function reference > matlab/graphics

Implemented: 0. To implement: 7. In a branch: 0. Out of scope: 39.

To implement:

- [ ] `colornone`
- [ ] `datachildren`
- [ ] `figuredeletedlistener`
- [ ] `getcrosssection`
- [ ] `is2D`
- [ ] `tex`
- [ ] `white`

Out of scope: `axescheck` (internal helper, not a user function), `bwcontr` (internal helper, not a user function), `capturescreen` (desktop, IDE, GUI, printing or interactive tool), `closereq` (desktop, IDE, GUI, printing or interactive tool), `copyoptionsfcn` (internal helper, not a user function), `dateTickPicker` (desktop, IDE, GUI, printing or interactive tool), `datetickstr` (internal helper, not a user function), `defaulterrorcallback` (internal helper, not a user function), `figcopytemplatelistener` (internal helper, not a user function), `figcopytemplatelistener_legacy` (internal helper, not a user function), `figureheaderdlg` (desktop, IDE, GUI, printing or interactive tool), `figureToolbarCreateFcn` (desktop, IDE, GUI, printing or interactive tool), `findprinters` (desktop, IDE, GUI, printing or interactive tool), `getFigureToolManager` (desktop, IDE, GUI, printing or interactive tool), `getprinttemplate` (desktop, IDE, GUI, printing or interactive tool), `handle2struct` (internal helper, not a user function), `hasbehavior` (internal helper, not a user function), `hgaddbehavior` (internal helper, not a user function), `hgbehaviorfactory` (internal helper, not a user function), `hgclose` (internal helper, not a user function), `hgdisp` (internal helper, not a user function), `hgfeval` (internal helper, not a user function), `hggetbehavior` (internal helper, not a user function), `hggetdisp` (internal helper, not a user function), `hgrc` (internal helper, not a user function), `hgsetdisp` (internal helper, not a user function), `openFigure` (desktop, IDE, GUI, printing or interactive tool), `parseChartInputs` (internal helper, not a user function), `parseparams` (internal helper, not a user function), `printjob` (desktop, IDE, GUI, printing or interactive tool), `printtables` (desktop, IDE, GUI, printing or interactive tool), `printtemplate` (desktop, IDE, GUI, printing or interactive tool), `regionpicker` (desktop, IDE, GUI, printing or interactive tool), `resetplotview` (desktop, IDE, GUI, printing or interactive tool), `saveFigure` (desktop, IDE, GUI, printing or interactive tool), `setprinttemplate` (desktop, IDE, GUI, printing or interactive tool), `struct2handle` (internal helper, not a user function), `vertexpicker` (desktop, IDE, GUI, printing or interactive tool), `vga` (desktop, IDE, GUI, printing or interactive tool)

## Not in the function reference > matlab/imagesci

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 3.

Out of scope: `hdf` (scientific file format (HDF/netCDF/CDF/FITS/TIFF) or web service), `hdf5` (scientific file format (HDF/netCDF/CDF/FITS/TIFF) or web service), `Tiff` (scientific file format (HDF/netCDF/CDF/FITS/TIFF) or web service)

## Not in the function reference > matlab/iofun

Implemented: 0. To implement: 1. In a branch: 0. Out of scope: 5.

To implement:

- [ ] `dataread`

Out of scope: `deployprefdir` (internal helper, not a user function), `matfinfo` (internal helper, not a user function), `memmap_data_handle_holder` (scientific file format (HDF/netCDF/CDF/FITS/TIFF) or web service), `partialpath` (internal helper, not a user function), `timercb` (internal helper, not a user function)

## Not in the function reference > matlab/lang

Implemented: 1. To implement: 9. In a branch: 0. Out of scope: 2.

To implement:

- [ ] `doclink`
- [ ] `dynamicprops`
- [ ] `handle`
- [ ] `lists`
- [ ] `message`
- [ ] `parallel_function`
- [ ] `precedence`
- [ ] `script`
- [ ] `spmd_feval`

Language keywords to check: `break`, `case`, `catch`, `continue`, `else`, `elseif`, `for`, `function`, `if`, `otherwise`, `parfor`, `return`, `switch`, `try`, `while`

Implemented: `display`

Out of scope: `checkSyntacticWarnings` (internal helper, not a user function), `ParallelException` (internal helper, not a user function)

## Not in the function reference > matlab/matfun

Implemented: 1. To implement: 1. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `ltitr`

Implemented: `normest1`

## Not in the function reference > matlab/ops

Implemented: 0. To implement: 2. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `paren`
- [ ] `punct`

## Not in the function reference > matlab/optimfun

Implemented: 0. To implement: 3. In a branch: 0. Out of scope: 10.

To implement:

- [ ] `optimplotfunccount`
- [ ] `optimplotfval`
- [ ] `optimplotx`

Out of scope: `addLink` (internal helper, not a user function), `callAllOptimOutputAndPlotFcns` (internal helper, not a user function), `callAllOptimOutputFcns` (internal helper, not a user function), `callAllOptimPlotFcns` (internal helper, not a user function), `createCellArrayOfFunctions` (internal helper, not a user function), `createProblemStruct` (internal helper, not a user function), `createSolverStruct` (internal helper, not a user function), `separateOptimStruct` (internal helper, not a user function), `validateopts_UseParallel` (internal helper, not a user function), `validOptimProblemStruct` (internal helper, not a user function)

## Not in the function reference > matlab/polyfun

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 7.

Out of scope: `automesh` (internal helper, not a user function), `pwch` (internal helper, not a user function), `qhull` (internal helper, not a user function), `splncore` (internal helper, not a user function), `xychk` (internal helper, not a user function), `xyzchk` (internal helper, not a user function), `xyzvchk` (internal helper, not a user function)

## Not in the function reference > matlab/scribe

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 13.

Out of scope: `defaultItemHitCallback` (internal helper, not a user function), `deselectall` (internal helper, not a user function), `getselectobjects` (internal helper, not a user function), `hgconvertunits` (internal helper, not a user function), `legendcolorbarlayout` (internal helper, not a user function), `legendinfo` (internal helper, not a user function), `scribealign` (internal helper, not a user function), `scribeccp` (internal helper, not a user function), `scribecursors` (internal helper, not a user function), `selectobject` (internal helper, not a user function), `snaptogrid` (internal helper, not a user function), `startscribeobject` (internal helper, not a user function), `startscribepinning` (internal helper, not a user function)

## Not in the function reference > matlab/sparfun

Implemented: 0. To implement: 4. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `delsq`
- [ ] `nested`
- [ ] `numgrid`
- [ ] `rjr`

## Not in the function reference > matlab/specgraph

Implemented: 0. To implement: 3. In a branch: 0. Out of scope: 10.

To implement:

- [ ] `contours`
- [ ] `ezgraph3`
- [ ] `vissuite`

Out of scope: `changeseriestype` (internal helper, not a user function), `colstyle` (internal helper, not a user function), `isplotchild` (internal helper, not a user function), `makebars` (internal helper, not a user function), `moviein` (audio/video devices or screen capture), `parseplotapi` (internal helper, not a user function), `plotchild` (internal helper, not a user function), `plotdoneevent` (internal helper, not a user function), `plotutils` (internal helper, not a user function), `usev6plotapi` (internal helper, not a user function)

## Not in the function reference > matlab/strfun

Implemented: 0. To implement: 1. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `setstr`

## Not in the function reference > matlab/timeseries

Implemented: 0. To implement: 1. In a branch: 0. Out of scope: 39.

To implement:

- [ ] `event`

Out of scope: `abstracttimemetadata` (internal helper, not a user function), `BasicArray` (internal helper, not a user function), `dataChangeEvent` (internal helper, not a user function), `hds` (internal helper, not a user function), `metadata` (internal helper, not a user function), `SimTimeseries` (internal helper, not a user function), `simulinkts2ts` (internal helper, not a user function), `timeseriesArray` (internal helper, not a user function), `tsAlignSizes` (internal helper, not a user function), `tsAnalyzeAbsTime` (internal helper, not a user function), `tsarrayFcn` (internal helper, not a user function), `tsChkTime` (internal helper, not a user function), `tsdata` (internal helper, not a user function), `tsdateinterval` (internal helper, not a user function), `tsgetDateFormat` (internal helper, not a user function), `tsgetrelativetime` (internal helper, not a user function), `tsinterp` (internal helper, not a user function), `tsIsDateFormat` (internal helper, not a user function), `tsIsSameTime` (internal helper, not a user function), `tsnaniqr` (internal helper, not a user function), `tsnanmax` (internal helper, not a user function), `tsnanmean` (internal helper, not a user function), `tsnanmedian` (internal helper, not a user function), `tsnanmin` (internal helper, not a user function), `tsnanmode` (internal helper, not a user function), `tsnanstd` (internal helper, not a user function), `tsnansum` (internal helper, not a user function), `tsnanvar` (internal helper, not a user function), `tsnewevent` (internal helper, not a user function), `tspnmatch` (internal helper, not a user function), `tsprctile` (internal helper, not a user function), `tspvformat` (internal helper, not a user function), `tssorttime` (internal helper, not a user function), `tsunitconv` (internal helper, not a user function), `utTrimNans` (internal helper, not a user function), `uttsget` (internal helper, not a user function), `ValueArray` (internal helper, not a user function), `variable` (internal helper, not a user function), `VirtualArray` (internal helper, not a user function)

## Not in the function reference > symbolic/symbolic

Implemented: 0. To implement: 7. In a branch: 0. Out of scope: 7.

To implement:

- [ ] `charToFunction`
- [ ] `frac`
- [ ] `getEqnsVars`
- [ ] `isCondition`
- [ ] `isDistinctVariable`
- [ ] `isVariable`
- [ ] `symbolicUnitsCollection`

Out of scope: `allMuPADNotebooks` (desktop, IDE, GUI, printing or interactive tool), `checkIgnoreAnalyticConstraintsValue` (internal helper, not a user function), `mupadengine` (desktop, IDE, GUI, printing or interactive tool), `mupadwelcome` (desktop, IDE, GUI, printing or interactive tool), `openmn` (desktop, IDE, GUI, printing or interactive tool), `openxvc` (desktop, IDE, GUI, printing or interactive tool), `openxvz` (desktop, IDE, GUI, printing or interactive tool)

## Symbolic Math Toolbox > Code Generation

Implemented: 1. To implement: 8. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `ccode`
- [ ] `fortran`
- [ ] `mathml`
- [ ] `matlabFunctionBlock`
- [ ] `simscapeEquation`
- [ ] `symReadSSCParameters`
- [ ] `symReadSSCVariables`
- [ ] `symWriteSSC`

Implemented: `latex`

## Symbolic Math Toolbox > Graphics

Implemented: 0. To implement: 5. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `animationToFrame`
- [ ] `fanimator`
- [ ] `playAnimation`
- [ ] `rewindAnimation`
- [ ] `writeAnimation`

## Symbolic Math Toolbox > Mathematics > Assumptions

Implemented: 2. To implement: 1. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `assumptions`

Implemented: `assume`, `assumeAlso`

## Symbolic Math Toolbox > Mathematics > Calculus

Implemented: 4. To implement: 23. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `changeIntegrationVariable`
- [ ] `fourier`
- [ ] `functionalDerivative`
- [ ] `funtool`
- [ ] `hessian`
- [ ] `htrans`
- [ ] `ifourier`
- [ ] `ihtrans`
- [ ] `ilaplace`
- [ ] `integrateByParts`
- [ ] `iztrans`
- [ ] `laplace`
- [ ] `pade`
- [ ] `potential`
- [ ] `rsums`
- [ ] `series`
- [ ] `symprod`
- [ ] `symsum`
- [ ] `taylortool`
- [ ] `vectorPotential`
- [ ] `vpaintegral`
- [ ] `vpasum`
- [ ] `ztrans`

Implemented: `int`, `jacobian`, `limit`, `taylor`

## Symbolic Math Toolbox > Mathematics > Equation Solving

Implemented: 0. To implement: 20. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `daeFunction`
- [ ] `dsolve`
- [ ] `eliminate`
- [ ] `equationsToMatrix`
- [ ] `findDecoupledBlocks`
- [ ] `finverse`
- [ ] `incidenceMatrix`
- [ ] `isLowIndexDAE`
- [ ] `isolate`
- [ ] `massMatrixForm`
- [ ] `odeFunction`
- [ ] `odeToVectorField`
- [ ] `pdeCoefficients`
- [ ] `pdeCoefficientsToDouble`
- [ ] `poles`
- [ ] `reduceDAEIndex`
- [ ] `reduceDAEToODE`
- [ ] `reduceDifferentialOrder`
- [ ] `reduceRedundancies`
- [ ] `vpasolve`

## Symbolic Math Toolbox > Mathematics > Formula Manipulation and Simplification

Implemented: 6. To implement: 7. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `combine`
- [ ] `displayFormula`
- [ ] `divisors`
- [ ] `horner`
- [ ] `partfrac`
- [ ] `simplifyFraction`
- [ ] `subexpr`

Implemented: `coeffs`, `collect`, `expand`, `lhs`, `numden`, `rhs`

## Symbolic Math Toolbox > Mathematics > Linear Algebra

Implemented: 0. To implement: 7. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `adjoint`
- [ ] `bernsteinMatrix`
- [ ] `charpoly`
- [ ] `colspace`
- [ ] `hermiteForm`
- [ ] `jordan`
- [ ] `smithForm`

## Symbolic Math Toolbox > Mathematics > Mathematical Functions

Implemented: 0. To implement: 55. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `catalan`
- [ ] `coshint`
- [ ] `cosint`
- [ ] `dawson`
- [ ] `dilog`
- [ ] `dirac`
- [ ] `ei`
- [ ] `ellipticCE`
- [ ] `ellipticCK`
- [ ] `ellipticCPi`
- [ ] `ellipticE`
- [ ] `ellipticF`
- [ ] `ellipticK`
- [ ] `ellipticNome`
- [ ] `ellipticPi`
- [ ] `erfi`
- [ ] `eulergamma`
- [ ] `fresnelc`
- [ ] `fresnels`
- [ ] `heaviside`
- [ ] `hurwitzZeta`
- [ ] `hypergeom`
- [ ] `igamma`
- [ ] `jacobiAM`
- [ ] `jacobiCD`
- [ ] `jacobiCN`
- [ ] `jacobiCS`
- [ ] `jacobiDC`
- [ ] `jacobiDN`
- [ ] `jacobiDS`
- [ ] `jacobiNC`
- [ ] `jacobiND`
- [ ] `jacobiNS`
- [ ] `jacobiSC`
- [ ] `jacobiSD`
- [ ] `jacobiSN`
- [ ] `jacobiZeta`
- [ ] `kroneckerDelta`
- [ ] `kummerU`
- [ ] `lambertw`
- [ ] `logint`
- [ ] `meijerG`
- [ ] `pochhammer`
- [ ] `polylog`
- [ ] `rectangularPulse`
- [ ] `signIm`
- [ ] `sinc`
- [ ] `sinhint`
- [ ] `sinint`
- [ ] `ssinint`
- [ ] `triangularPulse`
- [ ] `whittakerM`
- [ ] `whittakerW`
- [ ] `wrightOmega`
- [ ] `zeta`

## Symbolic Math Toolbox > Mathematics > Number Theory

Implemented: 0. To implement: 11. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `bernoulli`
- [ ] `euler`
- [ ] `eulerPhi`
- [ ] `factorIntegerPower`
- [ ] `fibonacci`
- [ ] `harmonic`
- [ ] `isPrimitiveRoot`
- [ ] `jacobiSymbol`
- [ ] `nextprime`
- [ ] `nthprime`
- [ ] `prevprime`

## Symbolic Math Toolbox > Mathematics > Numbers and Precision

Implemented: 1. To implement: 0. In a branch: 0. Out of scope: 0.

Implemented: `digits`

## Symbolic Math Toolbox > Mathematics > Polynomials

Implemented: 0. To implement: 14. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `bernstein`
- [ ] `chebyshevT`
- [ ] `chebyshevU`
- [ ] `gbasis`
- [ ] `gegenbauerC`
- [ ] `hermiteH`
- [ ] `jacobiP`
- [ ] `laguerreL`
- [ ] `legendreP`
- [ ] `minpoly`
- [ ] `polynomialDegree`
- [ ] `polynomialReduce`
- [ ] `resultant`
- [ ] `root`

## Symbolic Math Toolbox > MuPAD to MATLAB Migration

Implemented: 0. To implement: 0. In a branch: 0. Out of scope: 3.

Out of scope: `convertMuPADNotebook` (MuPAD to MATLAB Migration), `mupad` (MuPAD to MATLAB Migration), `symengine` (MuPAD to MATLAB Migration)

## Symbolic Math Toolbox > Symbolic Computations in MATLAB > Conversion Between Symbolic and Numeric

Implemented: 5. To implement: 2. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `cell2sym`
- [ ] `sym2cell`

Implemented: `matlabFunction`, `poly2sym`, `subs`, `sym2poly`, `vpa`

## Symbolic Math Toolbox > Symbolic Computations in MATLAB > Operators and Elementary Operations

Implemented: 14. To implement: 5. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `in`
- [ ] `powermod`
- [ ] `quorem`
- [ ] `symfalse`
- [ ] `symtrue`

Implemented: `and`, `ge`, `gt`, `isAlways`, `ldivide`, `le`, `lt`, `minus`, `ne`, `not`, `or`, `power`, `rdivide`, `times`

## Symbolic Math Toolbox > Symbolic Computations in MATLAB > Symbolic Variables, Expressions, Functions, and Preferences

Implemented: 5. To implement: 17. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `argnames`
- [ ] `findSymType`
- [ ] `fold`
- [ ] `formula`
- [ ] `has`
- [ ] `hasSymType`
- [ ] `isSymType`
- [ ] `mapSymType`
- [ ] `piecewise`
- [ ] `str2sym`
- [ ] `symfunmatrix`
- [ ] `symfunmatrix2symfun`
- [ ] `symFunType`
- [ ] `symmatrix`
- [ ] `symmatrix2sym`
- [ ] `sympref`
- [ ] `symType`

Implemented: `children`, `pretty`, `sym`, `symfun`, `syms`

## Symbolic Math Toolbox > Symbolic Computations in MATLAB > Units of Measurement

Implemented: 0. To implement: 19. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `baseUnits`
- [ ] `checkUnits`
- [ ] `derivedUnits`
- [ ] `findUnits`
- [ ] `isUnit`
- [ ] `mixedUnits`
- [ ] `newUnit`
- [ ] `newUnitSystem`
- [ ] `removeUnit`
- [ ] `removeUnitSystem`
- [ ] `rewrite`
- [ ] `separateUnits`
- [ ] `str2symunit`
- [ ] `symunit`
- [ ] `symunit2str`
- [ ] `unitConversionFactor`
- [ ] `unitConvert`
- [ ] `unitInfo`
- [ ] `unitSystems`

