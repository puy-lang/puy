// puy-test: test-fs
program DirFunctions;

// The directory functions of System.SysUtils return False when the OS refuses
// (spec/stdlib/System.SysUtils/Files.md). Only names without separators, so that the case runs
// alike on every OS; Puy runs it on the runner's file system in memory. ForceDirectories is in
// golden/host/sysutils_files: Delphi XE4 raises EInOutError for a name without a directory part.

uses
  System.SysUtils;

begin
  WriteLn(DirectoryExists('m515_a'), ' ', FileExists('m515_a'));
  WriteLn(CreateDir('m515_a'), ' ', CreateDir('m515_a'));
  WriteLn(DirectoryExists('m515_a'), ' ', FileExists('m515_a'));
  WriteLn(CreateDir('m515_b'));
  WriteLn(DeleteFile('m515_a'), ' ', DeleteFile('m515_missing'));
  WriteLn(RenameFile('m515_a', 'm515_c'), ' ', RenameFile('m515_missing', 'm515_d'));
  WriteLn(DirectoryExists('m515_a'), ' ', DirectoryExists('m515_c'));
  WriteLn(RemoveDir('m515_c'), ' ', RemoveDir('m515_c'), ' ', RemoveDir('m515_b'));
  WriteLn(DirectoryExists('m515_b'), ' ', DirectoryExists('m515_c'));
  WriteLn(ExtractFileExt('archive.tar.gz'), ' ', ExtractFileExt('noext'), ' ',
    ChangeFileExt('a.txt', '.pas'), ' ', ChangeFileExt('b', '.x'), ' ', ExtractFileName('plain'));
end.
