program EqualsHashCrlf;

// The fix of E0765 (equality.md 3.1) inserts the declaration with the line ending of the file
// (CR LF here).

type
  TOnlyEquals = class
  public
    function Equals(Obj: TObject): Boolean; override;
  end;

function TOnlyEquals.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

begin
end.
