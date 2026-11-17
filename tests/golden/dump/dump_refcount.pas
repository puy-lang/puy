program DumpRefcount;

// The instructions of the values counted by their contents (objects.md 14.3, 15.2, 15.3): a
// `TObject` local is copied with `AssignObj` and listed as `:obj`; a `TObject` global (`obj`)
// and field are written with the uncounted instructions between `AddRefObj` and `ReleaseObj`,
// and read through an uncounted temporary into `AssignObj`. A reference counted class is
// counted like a string (`…Ref`), and `RefCount` is an intrinsic.

type
  TBox = class(TRefObject)
  public
    Content: TObject;
  end;

var
  Shared: TObject;

procedure Run;
var
  B: TBox;
  O, P: TObject;
begin
  B := TBox.Create;
  O := B;
  P := TBox.Create;
  O := P;
  B.Content := P;
  Shared := B.Content;
  O := Shared;
  WriteLn(B.RefCount, Assigned(O), Assigned(P));
end;

begin
  Run;
  Shared := nil;
end.
