program InterfaceRefCount;

// `_AddRef`, `_Release` and `QueryInterface` (objects.md 7.1, 7.2): declaring one in an interface
// or a class is E3017, whose fix removes the declaration (and the class method's body in the same
// file); calling one is E3017 without a fix.

type
  ICounted = interface
    procedure Run;
  end;

  TCounted = class(TRefObject, ICounted)
  public
    procedure Run;
  end;


procedure TCounted.Run;
begin
end;

var
  C: ICounted;
  T: TCounted;
begin
  C._AddRef;
  T.QueryInterface;
end.
