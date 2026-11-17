unit Shapes;

interface

implementation

uses Log;

initialization
  Note('init Shapes');

finalization
  Note('fini Shapes');
  Flush(Output); // FPC 3.2.2 does not write out what finalization writes without it
end.
