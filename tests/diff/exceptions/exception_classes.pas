program ExceptionClasses;
// Exception classes with fields, constructors, methods and properties (objects.md 8.1-8.4):
// `inherited Create(Msg)` sets `Message`, a field and a method of the class are read in the
// handler, the handler chooses by the class of the instance, and `ExceptObject` read in a routine
// called from an `except` block is the exception being handled. (`ToString` is in
// golden/runtime/exception_to_string: FPC 3.2.2 returns the class name with the message.)

uses System.SysUtils;

type
  EAppError = class(Exception)
  private
    FCode: Integer;
  public
    constructor Create(const Msg: string; ACode: Integer);
    function Describe: string;
    property Code: Integer read FCode;
  end;

  ENetError = class(EAppError)
  public
    Host: string;
    constructor CreateHost(const AHost: string);
  end;

  EPlain = class(Exception)
  public
    Count: Integer;
  end;

constructor EAppError.Create(const Msg: string; ACode: Integer);
begin
  inherited Create(Msg);
  FCode := ACode;
end;

function EAppError.Describe: string;
begin
  Result := Message + ' (' + IntToStr(FCode) + ')';
end;

constructor ENetError.CreateHost(const AHost: string);
begin
  inherited Create('cannot reach ' + AHost, 7);
  Host := AHost;
end;

procedure LogCurrent;
begin
  WriteLn('log: ', ExceptObject.ClassName, ': ', (ExceptObject as Exception).Message);
end;

procedure Fail(N: Integer);
var
  P: EPlain;
begin
  case N of
    1: raise EAppError.Create('bad input', 42);
    2: raise ENetError.CreateHost('example.org');
    3:
      begin
        P := EPlain.Create('plain');
        P.Count := 3;
        raise P;
      end;
  end;
end;

procedure Run(N: Integer);
begin
  try
    Fail(N);
  except
    on E: ENetError do
    begin
      WriteLn('net: ', E.Describe, ' host=', E.Host);
      LogCurrent;
    end;
    on E: EAppError do
    begin
      WriteLn('app: ', E.Describe, ' code=', E.Code);
      LogCurrent;
    end;
    on E: EPlain do
      WriteLn('plain: ', E.Message, ' count=', E.Count);
  end;
end;

begin
  Run(1);
  Run(2);
  Run(3);
  WriteLn('done');
end.
