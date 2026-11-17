program RefcountLongChain;

// Letting go of the head of a list of a million instances destroys them all without
// `EStackOverflow` (objects.md 3.3: the destruction runs on the release jobs, not on the Rust
// stack, and is not counted toward the recursion limit). A class with a destructor of its own
// goes through its frame each time; one without is destroyed without a frame.

type
  TLink = class(TRefObject)
  public
    Next: TLink;
  end;

  TCountedLink = class(TRefObject)
  public
    Next: TCountedLink;
    destructor Destroy; override;
  end;

var
  Destroyed: Integer;

destructor TCountedLink.Destroy;
begin
  Destroyed := Destroyed + 1;
  inherited Destroy;
end;

procedure Plain;
var
  Head, N: TLink;
  I: Integer;
begin
  Head := nil;
  for I := 1 to 1000000 do
  begin
    N := TLink.Create;
    N.Next := Head;
    Head := N;
  end;
  N := nil;
  Head := nil;
  WriteLn('plain freed: ', Assigned(Head), Assigned(N));
end;

procedure WithDestructor;
var
  Head, N: TCountedLink;
  I: Integer;
begin
  Head := nil;
  for I := 1 to 100000 do
  begin
    N := TCountedLink.Create;
    N.Next := Head;
    Head := N;
  end;
  N := nil;
  Head := nil;
  WriteLn('destroyed: ', Destroyed, Assigned(Head), Assigned(N));
end;

begin
  Plain;
  WithDestructor;
end.
