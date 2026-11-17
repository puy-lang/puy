program DumpAnonCaptureMore;

// More dumps of captures (objects.md 15.5, 14.9): `Self` captured by an anonymous method in a
// method (`.capture #0 Self: … self`), a `TObject` variable in a cell (`NewCellObj`), a `const`
// string parameter copied into its cell (`CellSetRef`), and a nested routine value calling itself
// (`block link` inside the routine of the same group).

type
  TBox = class(TRefObject)
    Name: string;
    function Show: TProc;
  end;

function TBox.Show: TProc;
begin
  Result := procedure begin WriteLn(Name) end;
end;

function Make(const S: string): TProc;
var
  O: TObject;
  Total: Integer;

  procedure Down;
  begin
    Inc(Total);
    if Total < 2 then
      Down;
  end;

begin
  O := TBox.Create;
  Total := 0;
  Down;
  Result := procedure begin WriteLn(S, ' ', O.ClassName, ' ', Total) end;
  Result();
  Result := Down;
end;

var
  B: TBox;
  P: TProc;
begin
  B := TBox.Create;
  B.Name := 'box';
  P := B.Show();
  P();
  P := Make('s');
end.
