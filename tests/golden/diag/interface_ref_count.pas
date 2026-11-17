program InterfaceRefCount;

// `_AddRef`, `_Release` and `QueryInterface` (objects.md 7.1, 7.2): declaring one in an interface
// or a class is E3017, whose fix removes the declaration (and the class method's body in the same
// file); calling one is E3017 without a fix.

type
  ICounted = interface
    function _AddRef: Integer;
    procedure Run;
  end;

  TCounted = class(TRefObject, ICounted)
  public
    function _Release: Integer;
    procedure Run;
  end;

function TCounted._Release: Integer;
begin
  Result := 0;
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
