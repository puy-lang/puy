program ClassRefErrors;

// Errors through class references (objects.md 2.8, 3.1, 14.3, 14.4): creating from a `nil` class
// reference, calling a virtual class method on one, and the members of `TObject` about classes on
// `nil` are EAccessViolation; an abstract class can be created through a class reference (only
// creating it from its name is E0742), and calling its abstract method is EAbstractError, which
// names the method with the class that declares it. The last call leaves the exception unhandled,
// for the report and its trace.

uses
  System.SysUtils;

type
  TShape = class abstract
    function Area: Integer; virtual; abstract;
    class function Name: string; virtual;
  end;
  TBlob = class(TShape)
  end;
  TShapeClass = class of TShape;

class function TShape.Name: string;
begin
  Result := 'shape';
end;

procedure Try1(const What: string; C: TShapeClass; Step: Integer);
var
  S: TShape;
begin
  try
    case Step of
      0: begin
           S := C.Create;
           S.Free;
         end;
      1: WriteLn(C.Name);
      2: WriteLn(C.ClassName);
      3: WriteLn(C.ClassParent = nil);
    end;
    WriteLn(What, ': no error');
  except
    on E: Exception do WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

procedure Area(C: TShapeClass);
var
  S: TShape;
begin
  S := C.Create;
  try
    WriteLn(S.ClassName, ' created');
    WriteLn(S.Area);
  finally
    S.Free;
  end;
end;

begin
  Try1('create', nil, 0);
  Try1('class method', nil, 1);
  Try1('class name', nil, 2);
  Try1('class parent', nil, 3);
  try
    Area(TBlob);
  except
    on E: Exception do WriteLn(E.ClassName, ': ', E.Message);
  end;
  Area(TShape);
end.
