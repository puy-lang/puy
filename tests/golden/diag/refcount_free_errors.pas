program RefcountFreeErrors;

uses System.SysUtils;

// `Free`, `Destroy` and `FreeAndNil` of a reference counted class are E3019 (objects.md 3.2):
// a variable or a field gets the fix that assigns `nil` instead; `Self` (a bare `Free` in a
// method), a `const` parameter and a value that cannot be assigned get the note alone.

type
  TItem = class(TRefObject)
  public
    Next: TItem;
    procedure Drop;
  end;

var
  G: TItem;

procedure TItem.Drop;
begin
  Free;
  Destroy;
end;

procedure Release(const Item: TItem);
begin
  Item.Free;
end;

var
  X: TItem;
begin
  X := TItem.Create;
  X.Free;
  X.Destroy;
  X.Next.Free;
  G.Free;
  FreeAndNil(X);
  TItem.Create.Free;
end.
