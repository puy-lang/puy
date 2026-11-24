// puy-test: no-fs
program Capabilities;

// TCapabilities answers what the isolate's host has (spec/stdlib/Puy.Platform/README.md). The
// runner's host has a console and environment variables only; the others were not granted (the
// `puy` command, which has them all, does not run it).

uses
  Puy.Platform;

begin
  for var C := Low(TCapability) to High(TCapability) do
    WriteLn(C, ' ', TCapabilities.Status(C), ' ', TCapabilities.Has(C));
  WriteLn(TCapabilities.Console, ' ', TCapabilities.Environment, ' ', TCapabilities.Clock, ' ',
    TCapabilities.Random, ' ', TCapabilities.Memory, ' ', TCapabilities.FileSystem, ' ',
    TCapabilities.OS);
end.
