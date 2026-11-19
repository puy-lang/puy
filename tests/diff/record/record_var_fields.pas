program RecordVarFields;

// Fields of a record read and written through a reference: a `var` / `out` parameter, `Self` of
// a record method and a variable of an enclosing routine (bytecode.md 2.8, `RefFieldGet` /
// `RefFieldSet`). The variable may hold the default record (never written before), share its
// record with a copy, or be a global the callee also copies; the fields are plain, managed,
// a nested record and a dynamic array.

type
  TObj = class
    N: Integer;
  end;
  TInner = record
    X: Integer;
  end;
  TItem = record
    Count: Integer;
    Name: string;
    Inner: TInner;
    Arr: array of Integer;
    Obj: TObj;
  end;
  TPoint = record
    X, Y: Integer;
    procedure Move(DX, DY: Integer);
    function Sum: Integer;
  end;

var
  G: TItem;

procedure TPoint.Move(DX, DY: Integer);
begin
  X := X + DX;
  Y := Y + DY;
end;

function TPoint.Sum: Integer;
begin
  Result := X + Y;
end;

procedure Bump(var It: TItem; const S: string);
begin
  It.Count := It.Count + 1;
  It.Name := It.Name + S;
end;

// `It` is `G`: the copy taken inside keeps the values from before the writes
procedure CopyGlobal(var It: TItem);
var
  H: TItem;
begin
  H := G;
  It.Count := 100;
  It.Name := 'changed';
  WriteLn(H.Count, ' ', H.Name, ' ', G.Count, ' ', G.Name);
end;

// The default record written to `It` holds no block until a field is written
procedure Fill(out It: TItem);
begin
  It := Default(TItem);
  It.Count := 7;
  It.Name := 'out';
end;

procedure Deep(var It: TItem);
begin
  It.Inner.X := It.Inner.X + 5;
  SetLength(It.Arr, 3);
  It.Arr[1] := It.Count * 2;
  Inc(It.Count);
end;

procedure Own(var It: TItem; O: TObj);
begin
  It.Obj := O;
  It.Obj.N := It.Obj.N + 1;
end;

procedure Outer;
var
  R: TItem;

  procedure Inner;
  begin
    R.Count := R.Count + 10;
    R.Name := R.Name + 'i';
  end;

begin
  R := Default(TItem);
  Inner;
  Inner;
  WriteLn(R.Count, ' ', R.Name);
end;

var
  A, B: TItem;
  P, Q: TPoint;
  O: TObj;
begin
  Bump(A, 'a');
  WriteLn(A.Count, ' ', A.Name);
  B := A;
  Bump(A, 'b');
  WriteLn(A.Count, ' ', A.Name, ' / ', B.Count, ' ', B.Name);

  G.Count := 1;
  G.Name := 'g';
  CopyGlobal(G);
  WriteLn(G.Count, ' ', G.Name);

  Fill(B);
  WriteLn(B.Count, ' ', B.Name, ' / ', A.Count, ' ', A.Name);

  B := A;
  Deep(A);
  WriteLn(A.Count, ' ', A.Inner.X, ' ', Length(A.Arr), ' ', A.Arr[1], ' / ', B.Count, ' ',
    B.Inner.X, ' ', Length(B.Arr));

  O := TObj.Create;
  O.N := 3;
  Own(A, O);
  WriteLn(A.Obj.N, ' ', O.N, ' ', Ord(B.Obj = nil));
  O.Free;

  P.X := 1;
  P.Y := 2;
  Q := P;
  P.Move(10, 20);
  WriteLn(P.Sum, ' ', Q.Sum);

  Outer;
end.
