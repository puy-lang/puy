program MethodHeadingDirective;

// A method directive written on the heading of a method's body is E0401 (objects.md chapter 1):
// it belongs to the declaration in the class.

type
  TBase = class
    procedure Run; virtual;
  end;

procedure TBase.Run; virtual;
begin
end;

begin
end.
