A generated CMake root now builds only the packages its bringup's images reach,
through their launch files, owned entries and `<depend>`s. Before, it built every
package in the workspace, so a package belonging to another bringup could fail
this build or add executables to it. A package that uses another workspace package
without declaring it is refused, and the error names the `<depend>` to add to its
`package.xml`.
