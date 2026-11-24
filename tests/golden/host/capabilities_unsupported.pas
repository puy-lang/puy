// puy-test: test-fs, no-env, absent=unsupported
program CapabilitiesUnsupported;

// A target that lacks a capability reports it as Unsupported rather than NotPermitted
// (spec/stdlib/Puy.Platform/README.md); the file system and the OS of `test-fs` are there.

uses
  Puy.Platform;

begin
  for var C := Low(TCapability) to High(TCapability) do
    WriteLn(C, ' ', TCapabilities.Status(C));
  WriteLn(TCapabilities.FileSystem, ' ', TCapabilities.Environment);
end.
