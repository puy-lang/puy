// The members of an exception object before it is raised, assignment to `Message`, assignment
// compatibility with ancestors, and comparison by identity (docs/spec/exceptions.md 1, 2.1, 5.6).
type
  EAppError = class(Exception);
  EConfigError = class(EAppError);

function MakeError(const What: string): EAppError;
begin
  Result := EConfigError.Create('bad ' + What);
end;

var E: EAppError := MakeError('port');
var X: Exception := E;
var C: EConvertError := System.EConvertError.Create('not a number');
WriteLn(E.Message);
WriteLn(E.ClassName);
WriteLn(X.ClassName);
WriteLn(C.ClassName, ' [', C.Value, ']');
WriteLn('[', E.StackTrace, ']');
WriteLn(Length(E.Suppressed));
WriteLn(Assigned(E.InnerException));
X.Message := 'changed through X';
WriteLn(E.Message);
WriteLn(X = E, ' ', X <> E, ' ', X = nil, ' ', nil <> X);
WriteLn(Assigned(X));
X := nil;
WriteLn(Assigned(X), ' ', X = E);
E := nil;
WriteLn(Assigned(E));
WriteLn(Exception.Create('direct').Message);
