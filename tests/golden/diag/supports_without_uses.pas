program SupportsWithoutUses;

// `Supports` is declared in `System.SysUtils` (objects.md 7.2): without `uses` it is an
// undeclared identifier.

type
  IRun = interface
    procedure Run;
  end;

var
  O: TObject;
begin
  O := TObject.Create;
  WriteLn(Supports(O, IRun));
end.
