program ValueParamBorrow;

// A managed value parameter the routine does not write borrows its argument, like a `const` one
// (design/lifetime.md 3.3; M3-70): a global or a field passed to it is first copied into a
// temporary of the caller, so the callee letting go of the global or the field does not destroy
// the instance it reads, which lives until the caller's routine ends; a local is lent as it is.
// A parameter the routine writes counts its argument, so writing it neither changes nor frees the
// caller's value. Nothing leaks.

type
  TTag = class(TRefObject)
  private
    FName: string;
  public
    constructor Create(const AName: string);
    destructor Destroy; override;
    property Name: string read FName;
  end;

  THolder = class(TRefObject)
  public
    Tag: TTag;
  end;

constructor TTag.Create(const AName: string);
begin
  inherited Create;
  FName := AName;
end;

destructor TTag.Destroy;
begin
  WriteLn('destroy ', FName);
  inherited Destroy;
end;

var
  G: TTag;
  H: THolder;

procedure DropGlobal(T: TTag);
begin
  G := nil;
  WriteLn('still alive: ', T.Name);
end;

procedure DropField(T: TTag; O: TObject);
begin
  H.Tag := nil;
  WriteLn('still alive: ', T.Name, ' ', O <> nil);
end;

procedure Replace(T: TTag);
begin
  T := TTag.Create('replacement');
  WriteLn('replaced with ', T.Name);
end;

procedure Clear(T: TTag);
begin
  T := nil;
  WriteLn('cleared ', T = nil);
end;

procedure RunGlobal;
begin
  G := TTag.Create('global');
  DropGlobal(G);
  WriteLn('after DropGlobal');
end;

procedure RunField;
begin
  H := THolder.Create;
  H.Tag := TTag.Create('field');
  DropField(H.Tag, H.Tag);
  WriteLn('after DropField');
end;

procedure RunLocal;
var
  L: TTag;
begin
  L := TTag.Create('local');
  Replace(L);
  Clear(L);
  WriteLn('local is ', L.Name);
end;

begin
  RunGlobal;
  WriteLn('-');
  RunField;
  WriteLn('-');
  RunLocal;
  WriteLn('end');
end.
