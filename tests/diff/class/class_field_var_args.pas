program ClassFieldVarArgs;

// Fields of an instance passed to `var` / `out` parameters (objects.md 2.2): the callee's writes
// reach the field, two fields of the same instance can be passed together (`Swap(O.X, O.Y)`),
// a field of a record field and a managed field are written back, and `Inc` / `Dec` change a
// field in place. `FreeAndNil` (objects.md 3.2) takes a variable, a field or an array element of
// any class type, frees the instance and leaves `nil` (`Assigned` is `False`); `nil` is freed as
// nothing.

uses
  System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
  end;

  TNode = class
  public
    X, Y: Integer;
    Name: string;
    Pos: TPoint;
    Next: TNode;
    destructor Destroy; override;
  end;

  TLeaf = class(TNode)
  end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited Destroy;
end;

procedure Swap(var A, B: Integer);
var
  T: Integer;
begin
  T := A;
  A := B;
  B := T;
end;

procedure Append(var S: string; const Tail: string);
begin
  S := S + Tail;
end;

procedure Clear(out N: Integer);
begin
  N := 0;
end;

procedure Shift(var P: TPoint; D: Integer);
begin
  P.X := P.X + D;
  P.Y := P.Y - D;
end;

var
  O: TNode;
  L: TLeaf;
  Arr: array of TNode;
begin
  O := TNode.Create;
  O.Name := 'first';
  O.X := 1;
  O.Y := 2;
  Swap(O.X, O.Y);
  WriteLn(O.X, ' ', O.Y);
  Inc(O.X);
  Dec(O.Y, 5);
  WriteLn(O.X, ' ', O.Y);
  Append(O.Name, '!');
  Append(O.Name, '?');
  WriteLn(O.Name);
  Clear(O.X);
  WriteLn(O.X);
  O.Pos.X := 10;
  Swap(O.Pos.X, O.Pos.Y);
  WriteLn(O.Pos.X, ' ', O.Pos.Y);
  Shift(O.Pos, 3);
  WriteLn(O.Pos.X, ' ', O.Pos.Y);
  O.Next := TNode.Create;
  O.Next.Name := 'second';
  O.Next.X := 4;
  Swap(O.X, O.Next.X);
  WriteLn(O.X, ' ', O.Next.X);
  FreeAndNil(O.Next);
  WriteLn(Assigned(O.Next));
  FreeAndNil(O.Next);
  L := TLeaf.Create;
  L.Name := 'leaf';
  FreeAndNil(L);
  WriteLn(Assigned(L));
  SetLength(Arr, 2);
  Arr[1] := TNode.Create;
  Arr[1].Name := 'element';
  FreeAndNil(Arr[1]);
  WriteLn(Assigned(Arr[1]));
  FreeAndNil(O);
  WriteLn(Assigned(O), ' ', O = nil);
end.
