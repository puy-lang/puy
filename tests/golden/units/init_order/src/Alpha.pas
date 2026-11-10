unit Alpha;

interface

uses Gamma;

implementation

uses Delta;

initialization
  WriteLn('init Alpha');
finalization
  WriteLn('fini Alpha');
end.
