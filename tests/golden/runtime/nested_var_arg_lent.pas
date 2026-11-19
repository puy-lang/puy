program NestedVarArgLent;

uses
  System.SysUtils;

// A nested routine that takes the static link (it uses a variable of the routine it is nested
// in) gets a field or element passed to its `var` / `out` parameter lent to the call: the link
// comes right before the arguments, after the registers of the lending (objects.md 9.7,
// bytecode.md 2.8). Also: a closure-form nested routine, which gets its group's block first.

type
  TPoint = record
    X, Y: Integer;
  end;

  TBox = class
    N: Integer;
    procedure Fill(V: Integer);
  end;

var
  G: TArray<Integer>;
  R: TPoint;
  B: TBox;
  Pts: TArray<TPoint>;
  S: TArray<string>;

procedure TBox.Fill(V: Integer);
  procedure Put(var Dst: Integer);
  begin
    Dst := V;
  end;
begin
  // A field of `Self`
  Put(N);
end;

procedure Elements(V: Integer);
  procedure Ins(var Dst: Integer);
  begin
    Dst := V;
  end;
  procedure Pt(var P: TPoint);
  begin
    P.X := V;
    P.Y := V + 1;
  end;
  procedure Clear(out T: string);
  begin
    T := 'out' + IntToStr(V);
  end;
begin
  // An element of a dynamic array
  Ins(G[0]);
  // A field of a record
  Ins(R.Y);
  // A field of an instance
  Ins(B.N);
  // A flat record element, as a whole
  Pt(Pts[1]);
  // An `out` parameter of a managed type
  Clear(S[0]);
end;

function Twice(V: Integer): Integer;
  procedure Ins(var Dst: Integer);
  begin
    Dst := V * 2;
  end;
begin
  // The nested routine of a function
  Ins(G[1]);
  Result := G[1];
end;

procedure Grouped(V: Integer);
  procedure Ins(var Dst: Integer);
  begin
    Dst := V;
  end;
var
  P: TProc;
begin
  // `Ins` is compiled as a closure because the anonymous method calls it
  P := procedure
    begin
      Ins(G[2]);
    end;
  Ins(G[3]);
  P();
end;

begin
  SetLength(G, 4);
  SetLength(Pts, 2);
  SetLength(S, 1);
  B := TBox.Create;
  Elements(3);
  WriteLn(G[0], ' ', R.Y, ' ', B.N, ' ', Pts[1].X, ' ', Pts[1].Y, ' ', S[0]);
  WriteLn(Twice(4));
  Grouped(5);
  WriteLn(G[2], ' ', G[3]);
  B.Fill(7);
  WriteLn(B.N);
  B.Free;
end.
