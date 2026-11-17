program DumpClass;

// The dumps of classes (objects.md chapter 15): the `.type` lines with their fields (slot numbers
// after the parent's), `create … builtin`, `objfield` reads and targets, `free` (and `destroy`)
// in the `hir`; `New`, `ObjGet` / `ObjSetRef` and the `Free` / `Destroy` intrinsics in the
// `ir` / `bytecode`; and the `class` lines of the type section.

type
  TPoint = record
    X, Y: Integer;
  end;
  TBase = class
    Name: string;
  end;
  TChild = class(TBase)
    Count: Integer;
    Pos: TPoint;
    Next: TChild;
  end;

var
  C: TChild;
  O: TObject;
begin
  C := TChild.Create;
  C.Name := 'a';
  C.Count := C.Count + 1;
  C.Pos.Y := 2;
  WriteLn(C.Name, C.Pos.Y, Assigned(C.Next), C <> nil);
  O := TObject.Create;
  O.Destroy;
  C.Free;
end.
