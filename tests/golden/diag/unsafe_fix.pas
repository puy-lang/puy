program UnsafeFix;

// E3018 (attributes.md 3, objects.md 4.4): `[unsafe]` where `[weak]` fits (a field or variable
// of a reference counted class) gets the safe fix that renames it to `weak`; elsewhere a
// suggestion removes it, with a note for a class managed by hand, for a parameter, for a place
// that is not a field or variable (a type, a method, a property), and for any other type. The removal takes the whole list and the space after it when the attribute is
// alone in its brackets; otherwise the attribute with the `,` after it, or the last one with the
// `,` before it (the `.json` has the edits).

type
  TShape = class
  end;
  TNode = class(TRefObject)
  public
    [unsafe] Parent: TNode;
    [unsafe] Shape: TShape;
    [unsafe, weak] Items: TArray<TNode>;
    [weak, unsafe] Text: string;
    [unsafe] procedure Run;
    [unsafe] property Up: TNode read Parent;
  end;
  [unsafe] TNodes = TArray<TNode>;
  TRec = record
    [unsafe] Name: string;
  end;

procedure TNode.Run;
begin
end;

procedure Show(const [unsafe] N: TNode);
begin
  WriteLn(N <> nil);
end;

var
  [unsafe] Root: TNode;
  [unsafe] Any: TObject;
begin
  var [unsafe] Last: TNode := nil;
  Root := Last;
  Any := nil;
  Show(Root);
end.
