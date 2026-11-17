program ExceptionClassMembers;
// Exception classes as classes (objects.md 8.1, 8.2): `inherited CreateFmt` in a constructor,
// `AfterConstruction` and a destructor of an exception class (the destructor runs when the last
// reference goes, after the handler), creation through a class reference of an exception class,
// `is` / `as` on exceptions, and an uncaught exception of a declared class.

uses System.SysUtils;

type
  ETracked = class(Exception)
  public
    Tag: string;
    constructor CreateTag(const ATag: string; N: Integer);
    procedure AfterConstruction; override;
    destructor Destroy; override;
  end;

  ESub = class(ETracked);
  TExceptionClass = class of Exception;

constructor ETracked.CreateTag(const ATag: string; N: Integer);
begin
  inherited CreateFmt('%s #%d', [ATag, N]);
  Tag := ATag;
end;

procedure ETracked.AfterConstruction;
begin
  WriteLn('after construction: ', Message);
end;

destructor ETracked.Destroy;
begin
  WriteLn('destroy ', Tag);
  inherited Destroy;
end;

procedure Handle;
begin
  try
    raise ESub.CreateTag('a', 1);
  except
    on E: ETracked do
      WriteLn('caught ', E.ClassName, ' ', E.Message, ' ', E is ESub);
  end;
  WriteLn('handled');
end;

procedure ByReference(C: TExceptionClass);
var
  E: Exception;
begin
  E := C.Create('made by ' + C.ClassName);
  WriteLn(E.ClassName, ': ', E.Message);
end;

begin
  Handle;
  ByReference(Exception);
  ByReference(ESub);
  raise ETracked.CreateTag('last', 2);
end.
