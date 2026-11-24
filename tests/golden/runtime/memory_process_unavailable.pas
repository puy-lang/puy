// puy-test: absent=unsupported
// The suite runner's host has no MemoryStats. Run only there, since the CLI supplies MemoryStats.
program MemoryProcessUnavailable;

uses Puy.Runtime;

begin
  WriteLn(TMemory.Used, ' ', TMemory.Peak, ' ', TMemory.Limit, ' ', TMemory.Available);
end.
