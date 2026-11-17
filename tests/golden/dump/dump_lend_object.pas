program DumpLendObject;

// The dumps of a field of an instance passed to a `var` parameter (objects.md 14.3, 15): the
// handle is the root of `Lend` and the slot number starts its path, for the field itself and
// for a field of a record field; `FreeAndNil` copies the variable or field, writes `nil` there
// and frees the copy, without lending (hir `free and-nil`, ir).

uses
  System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
  end;

  TNode = class
  public
    Count: Integer;
    Pos: TPoint;
    Next: TNode;
  end;

procedure Bump(var N: Integer);
begin
  N := N + 1;
end;

var
  O: TNode;
begin
  O := TNode.Create;
  Bump(O.Count);
  Bump(O.Pos.Y);
  WriteLn(O.Count, ' ', O.Pos.Y);
  O.Next := TNode.Create;
  FreeAndNil(O.Next);
  FreeAndNil(O);
end.
