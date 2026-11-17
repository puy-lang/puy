program ClassFieldWriteOrder;

uses System.SysUtils;

// A write to a field of an instance evaluates the instance before the right-hand side, also
// when the field holds a record or an array written inside (objects.md 2.2): a call on the right
// that changes the variable holding the instance does not move the write to the other instance.

type
  TPoint = record
    X: Integer;
  end;
  TItem = class
    Name: string;
    Count: Integer;
    Pos: TPoint;
    Values: array of Integer;
    Fixed: array[0..1] of Integer;
  end;

var
  G, First, Second: TItem;

function Switch(V: Integer): Integer;
begin
  G := Second;
  Result := V;
end;

procedure Show(const What: string; O: TItem);
begin
  WriteLn(What, ': ', O.Count, ' ', O.Pos.X, ' ', O.Values[0], ' ', O.Fixed[1], ' [', O.Name, ']');
end;

begin
  First := TItem.Create;
  Second := TItem.Create;
  SetLength(First.Values, 1);
  SetLength(Second.Values, 1);
  G := First;
  G.Count := Switch(1);
  G := First;
  G.Pos.X := Switch(2);
  G := First;
  G.Values[0] := Switch(3);
  G := First;
  G.Fixed[1] := Switch(4);
  G := First;
  G.Name := G.Name + IntToStr(Switch(5));
  Show('first', First);
  Show('second', Second);
  First.Free;
  Second.Free;
end.
