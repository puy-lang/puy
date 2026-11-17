program MethodRefNoTempWarning;

// W0023 is only for a method reference whose receiver is a creation expression (objects.md 9.3):
// a function result and the result of `as` are not warned about (another place may hold the
// instance), nor is a creation expression thrown away in a statement. Only the one warning on
// `TBase.Create.Show` is expected.

type
  TEvent = procedure of object;

  TBase = class(TRefObject)
    procedure Show;
  end;

procedure TBase.Show;
begin
  WriteLn('show');
end;

var
  Keep: TBase;

function Mk: TBase;
begin
  Result := Keep;
end;

var
  O: TObject;
  E: TEvent;
begin
  Keep := TBase.Create;
  O := Keep;
  E := Mk.Show;
  E := (O as TBase).Show;
  TBase.Create;
  E := TBase.Create.Show;
  Keep := nil;
  O := nil;
end.
