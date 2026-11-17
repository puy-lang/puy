program WeakFlow;

// Definite assignment applies to `[weak]` variables and class references as to other locals
// (objects.md 4.4, semantics.md 4.4): reading one before it is assigned is E2000, and the fix
// adds the initial value `nil` (not `0`). Writing a `[weak]` variable that is never read is
// W0001.

type
  TNode = class(TRefObject)
  end;

  TNodeClass = class of TNode;

procedure Read;
var
  [weak] V: TNode = nil;
  K: TNodeClass = nil;
begin
  WriteLn(V = nil, K = nil);
end;

procedure Unused;
var
  [weak] W: TNode;
begin
  W := nil;
end;

begin
  Read;
  Unused;
end.
