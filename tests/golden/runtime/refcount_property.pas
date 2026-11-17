program RefcountProperty;

// `RefCount` is the number of references to an instance of a reference counted class, the
// temporaries of the expression included (objects.md chapter 5).

type
  TItem = class(TRefObject)
  end;

procedure Run;
var
  A, B: TItem;
begin
  A := TItem.Create;
  WriteLn(A.RefCount);
  B := A;
  WriteLn(A.RefCount, ' ', B.RefCount);
  B := nil;
  WriteLn(A.RefCount, ' ', Assigned(B));
end;

begin
  Run;
end.
