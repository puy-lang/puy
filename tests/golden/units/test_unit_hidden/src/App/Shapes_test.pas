// What a test unit does not see of its paired unit: the `strict private` and `strict protected`
// members (E0743), and the units the paired unit uses (E0700). A unit it uses itself shows only
// its public part, as to any other unit (E0700, E0743). A test unit has no `initialization` or
// `finalization` section (E0780; units.md 2.4). A class inheriting from the paired unit's class
// cannot redeclare its `private` field, as inside the paired unit (E0701), but can reuse the
// name of a `strict private` one.
unit App.Shapes_test;

interface

implementation

uses
  App.Shapes,
  App.Store;

type
  TSquare = class(TShape)
    FSide: Integer;
    FSecret: Integer;
  end;

[Test]
procedure Strict;
var
  S: TShape;
begin
  S := TShape.Create;
  S.FSide := 1;
  S.FSecret := 1;
  S.Hidden;
  S.Free;
end;

[Test]
procedure Transitive;
begin
  Assert(Logged = 0);
end;

procedure Other;
var
  I: TItem;
begin
  I := nil;
  Assert(StoreOnly = 2);
  Assert(I.FHidden = 0);
end;

initialization
  Other;
finalization
end.
