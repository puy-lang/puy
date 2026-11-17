program WeakErrors;

// E0748 (objects.md 4.4, 16): `[weak]` on a class managed by hand, on exactly `TObject` and on a
// type that is not a class (each with its note), and `[weak]` where only fields and variables
// take it (a parameter, a type, a method, a property). A language attribute takes no arguments
// (E0705; empty parentheses are none). A `[weak]` variable or field is not passed to a `var` /
// `out` parameter (E0706 with a note). A qualified name is not the language attribute (W0025
// when nothing has the name), and `[Test]` is E3200 until M5 (attributes.md 2, 5).

uses System.SysUtils;

type
  TShape = class
  end;
  TNode = class(TRefObject)
  public
    [weak] Shape: TShape;
    [weak] Any: TObject;
    [weak] Name: string;
    [weak] Items: TArray<TNode>;
    [weak(1)] Parent: TNode;
    [weak()] Next: TNode;
    [Test] Count: Integer;
    [System.Weak] Prev: TNode;
    [weak] procedure Run;
    [weak] property First: TNode read Next;
  end;
  [weak] TNodes = TArray<TNode>;

procedure TNode.Run;
begin
end;

procedure Show([weak] N: TNode);
begin
  WriteLn(N <> nil);
end;

procedure Change(var N: TNode);
begin
  N := nil;
end;

procedure Make(out N: TNode);
begin
  N := TNode.Create;
end;

var
  [weak] G: TNode;
  [weak] I: Integer;
  N: TNode;
begin
  N := TNode.Create;
  G := N;
  Change(G);
  Make(N.Next);
  FreeAndNil(G);
  Show(N);
end.
