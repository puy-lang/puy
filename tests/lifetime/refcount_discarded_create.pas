program RefcountDiscardedCreate;

// An instance of a reference counted class created in an expression statement and thrown
// away is held by the statement's temporary and destroyed when the routine ends (objects.md
// 4.1; design/lifetime.md 3.4 A).

type
  TNote = class(TRefObject)
  public
    destructor Destroy; override;
  end;

destructor TNote.Destroy;
begin
  WriteLn('destroy');
  inherited Destroy;
end;

procedure Run;
begin
  TNote.Create;
  WriteLn('created');
end;

begin
  Run;
  WriteLn('end');
end.
